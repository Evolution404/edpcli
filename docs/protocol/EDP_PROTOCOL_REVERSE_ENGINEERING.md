# EDP LBA0–LBA12 协议逆向与验证总文档

> **唯一真相源（唯一事实源）**
>
> 本文档汇总 Phase 0 以来 LBA0–LBA12 的当前结论、逐字节账本、写入端/消费端、
> 真实盘证据、官方/历史二进制取证、已证伪假设和剩余阻塞项。
> `COMPLETE` 只允许由可复核证据升级；`PARTIAL` 表示边界/部分语义已经验证但仍有
> 明确缺口；任何候选解释必须标注为候选或已证伪，不得写成事实。
>
> **当前 EDP 字节语义闭环进度：6656 / 6656B 完全闭环（100.0%），部分闭环=0，未知=0。**
> `COMPLETE` 与“每个已知配置类型都有真实物理正例”是两个独立维度；物理配置类型覆盖见 `audit/protocol/profile_coverage.tsv`。
>
> 文末“验证历程附录”用于保留详细推导和纠错记录；若附录中的历史阶段判断与本文前半
> 标准账本冲突，**一律以前半当前账本为准**。


> 目的：把 LBA0–12 的每一个已识别字段追到“官方生产者 → 盘面字节 → 官方消费者 → 实盘验证”，并以严格完成率衡量逆向进度。
>
> 本文是长期维护的**唯一协议分析主账本**。研究过程、历史误判、专项取证和更长的
> 证据讨论已全部迁入本文第10–12节，不再维护并行分析文档。

## 1. 完成判定：语义闭环与物理配置类型覆盖分层

字段的**语义状态**只有三种：

- **完全闭环**：字段边界、写入端/序列化或明确的调用方负责输入边界、消费端/行为语义已经闭合；若字段值本身由协议规定可推导算法，则该算法也必须闭合。真实原盘仍是最高价值的正向证据，但当某个配置类型缺少真实设备采集时，若一方官方二进制已直接生成该配置类型的正向盘面、消费端可独立正向消费且算法/边界可重放，该字段可以在**语义维度**保持完全闭环，同时必须把该配置类型单独标记为 `MISSING_PHYSICAL`，不得把虚拟输出写成物理采集。对于历史配置类型，若精确写入端二进制已经不可得，也允许在更严格的盘面级门槛下闭合“序列化语义”：必须有多个真实物理正向盘面、至少两个独立一方消费端对同一格式给出一致且唯一的逆映射、动态接缝/终止条件可精确定位，并且 NUL 后/保留底层字节等未消费区域的生命周期单独闭合；这种完全闭环只说明盘面序列化映射已确定，**不等于恢复了当年的生成可执行文件或上游配置类型选择器**。对于明确的调用方负责身份/材料，必须证明写入端只负责透明接收/序列化、消费端不依赖某个未证明的隐藏派生关系，此时更上游“业务为何选择这个值”的来源不属于盘面字段语义缺口。存在代际/配置类型差异时，差异也必须解释到不会影响字段语义。
- **部分闭环**：至少一项关键语义证据缺失。例如只有字段名、只有写入端、只有消费端、只有样本规律、只有解密公式，均只能算部分闭环。
- **未知**：尚不能稳定划定语义边界，或只知道“当前样本为零/固定值”。

与上述状态独立，`audit/protocol/profile_coverage.tsv` 维护已知配置类型的**物理正例覆盖**：`COVERED` 表示有对应真实设备正向采集；`MISSING_PHYSICAL` 表示当前只有静态/一方虚拟正向证据。语义完全闭环 **不等于**所有配置类型已完成物理采样，物理覆盖也不能替代写入端/消费端语义证明。

以下内容**永远不能单独把字段升级为完全闭环**：

1. 金标样本全部相同、全零或固定值；
2. DWARF/变量名看起来合理；
3. 可以正确解密；
4. edpcli 制盘可以生成；
5. 只有写入端没有读取端；
6. 只有读取端、没有写入端/真实盘面正例、也没有满足上述“双一方消费端唯一逆映射”历史门槛；
7. 免密转换结果、自生成盘或实验盘与预期一致。

### 1.1 实盘证据规则

从 2026-09-21 起，协议分析的 **通用统计集金标字节集**固定为仓库内
`audit/protocol/gold/`，确保干净克隆/CI/后续 AI 不依赖采集机本地目录即可重放：

- `audit/protocol/gold/strict-encrypted/`：19 份 SHA-256 唯一的 6656B 严格
  原始代际加密原盘 LBA0-LBA12；
- `audit/protocol/gold/authentic-nopwd/`：1 份 6656B 的 SanDisk Ultra 真实免密盘
  LBA0-LBA12，只读采集，作为免密行为/配置类型的唯一金标。

少数配置类型需要独立的特定用途真实设备正向，但不应改变通用统计集人口定义。这类证据统一放在 `audit/protocol/physical-evidence/`，必须有独立来源、SHA-256、只读采集边界和定向回归测试。例如 LBA10 EESI 启用配置类型的 `P-EESI-NETAC` 就属于这一类；它可以关闭对应配置类型的物理正向缺口，但不会被计成通用统计集的“第21份样本”。

原始采集来源仍分别保留为 `/Users/zhangyuxi/.edpcli-backup` 与
`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4`，但这两个目录已不再
是审计运行依赖。其中 `.edpcli-backup` 的 `_nopwd_` 文件是 edpcli 自制免密盘，只能
用于产品回归，**没有协议参考价值**，不得与真实免密金标混用。
`nopwd_tool/backup`、`utils/backup`、散落的历史快照以及仓库裁剪夹具均不得再作为
金标统计来源。仓库 `tests/fixtures/protocol` 只保留从金标提取的 CI 回归子集；附录中
残留的 22/57/58 份历史统计集仅记录当时研究过程，不能覆盖本节口径，也不能独立
支撑完全闭环。标准账本中涉及真实盘的结论必须能回到上述 19+1 通用统计集
或清单跟踪 的特定用途物理证据；否则不得把普通历史快照、
实验盘或临时目录样本冒充物理正向闭环。

**禁止把免密转换盘、自生成盘、文件名带 `_nopwd` 的夹具、或内容已经呈现自制免密
状态的备份作为“原始写入端协议”证据。**
**金标按完整 6656B SHA-256 去重；字节完全相同的重复只读采集只保留一份，不得按
独立真实盘重复计权。**

### 1.2 可复现审计基线（2026-09-21）

为防止“文档结论已更新、临时测试框架 / 样本口径仍停留在旧阶段”，仓库新增
`audit/protocol/` 作为本文的机器可校验伴随账本，而不是第二份真相源：

- `gold_samples.tsv`：冻结当前19份唯一严格加密原盘 + 1份真实免密盘 LBA0-LBA12
  的来源名、仓库相对路径、长度和 SHA-256；`audit/protocol/gold/` 保存对应完整
  6656B 字节，因此基线审计从干净克隆即可直接重放；
- `evidence_manifest.tsv`：把物理 / 虚拟 / 静态三类证据分开记录，固定
  官方二进制版本、SHA-256、关键函数地址、实验边界和不能证明的内容；
- `byte_ledger.tsv`：按配置类型标注并覆盖全部6656B，状态表示 EDP 字节**语义闭环**；
- `profile_coverage.tsv`：把已知配置类型的真实设备正向覆盖与语义状态分开维护，禁止把官方虚拟输出冒充物理采集；
- `historical_matrix.tsv`：集中记录历史 DLL 版本与 join59、HSerialCRC、
  UsbOnlyInfo、动态 MBR 四类指纹的命中/排除结果；
- `scripts/protocol/audit_baseline.py`：取代旧 `/private/tmp/audit22` 的样本统计集
  作用，只按本节两类金标重放；旧测试框架的来源 SHA-256
  `c9fb7ba50d715e8c0d53611c73076b3c50e7b40a23f05693001d33c17f05abba`
  仅保留为 lineage 记录；
- `tests/protocol_byte_ledger.rs`：自动展开所有范围，拒绝遗漏、重叠、证据 ID
  漂移和 6000/656 统计偏差。

本轮实际重放现行20份通用统计集唯一金标后：LBA10 **20/20 整扇全零**；LBA3 为19份全零 +
1份严格 Kingston 非零配置类型，后者 `+0x020..0x027=b57e9c4500800014`、
`+0x1F0..0x1FF="this is mp mark\0"`。旧 `/tmp/audit22` 曾混入的第三来源 SanDisk
EESI 正例仍不得作为通用统计集的第21份样本。

但这不再等同于“没有合格的 EESI 正向物理证据”。本轮重新审计
`/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`：完整6656B
SHA-256=`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`，companion
元数据固定 `device_id=disk&ven_netac&prod_onlydisk&rev_0000`、CRC32=`5088ee37`、
大小=6656、MD5=`db17edf8246ad55e9800b36701afd8e4`。产生这组三件套的
`make_big_boot.py`（审计时 SHA-256=`d72f6fcd192e92e6423e3b078cffa46725d8e781cdbd48dfcdaa470b72d208bd`）
在 `read_lba()` 中以 `O_RDONLY/pread` 读取，`--apply` 主流程先调用 `backup()` 保存
LBA0-LBA12，再进行第二次 `YES` 确认，之后才 unmount 并通过 `O_RDWR/pwrite` 修改
LBA0/LBA12。因此该文件是对应运行的**写前真实物理快照**。仓库现将它完整保存为
`audit/protocol/physical-evidence/eesi/netac_onlydisk_20260804_lba0_12.bin`，登记为
`P-EESI-NETAC`，只作为 EESI 启用配置类型的特定用途物理证据，不纳入19+1 通用统计集。
对其重放得到 `CRC32(device_id)=0x5088EE37`，LBA10 前0x80解密为 `EESI`、标志=1、
GBK“交换区”、GBK“保密区”、88B零兼容扩展；它与独立旧 SanDisk
EESI 正例的明文布局一致。结合两套官方 EESI 写入端/消费端生命周期，LBA10
`0x000..0x07F` 的真实设备门禁现已满足并升级完全闭环。

### 1.3 去重金标交叉复核：指纹不能合并为单一筛选条件

对当前仓库19份严格加密金标重新解码 LBA4/LBA6/LBA8，
`tests/protocol_gold_crosscheck.rs` 固定以下统计集（不再沿用旧22份计数）：

| 观察项 | 当前19份加密金标 |
|---|---|
| HSerialCRC[5] | 6份全零、12份固定 `1D29/7B/4DD/79/7C`、1份其它非零 |
| Dept | 12份短串、3份 join59、4份 join60 |
| LBA6 `+0x1E0..+0x1ED` | 18份全零、1份非零（Aigo U335 rev_pmap） |
| LBA0 前400B | 8份全零、10份 UsbMainBSec、1份 Netac |
| LBA4 MyHardinfo / LBA8 HDSerialInfo | 19/19 相等；所有样本 LBA4 保护值与 LBA6 校验和均通过 |

3份 join59 均携带固定 HSerial，且 MBR 片段为零；唯一非零片段样本
则是短 Dept + 其它非零 HSerial。**这不证明它们必然来自不同写入端**，但表明目前没有
同时展示 join59 与非零片段的金标。因此历史组件矩阵的四类指纹应作为独立
调查入口，不能要求候选同时命中四项才予保留。LBA0 模板同样不是 HSerial 世代的
单值判别器：当前全零 HSerial 与旧版非零 HSerial 都存在 UsbMainBSec/全零
引导代码实例。

历史写入端的获取目标也已从“未知中间代”收紧到可复核的本机运行基线。`Product_audit` 与 `VUpdateReplace.log` 已证明 2025-05-13 部署了 CEMS 基础版本 `8.1.2502.2116`，且其更新树包含 `ydcc/cemsusbregsiter.dll(.zip)`；新增审计 `VUpdateService.log`（SHA-256=`5ec3b53e34573646b29dde6cee5595fccb527c385f0b0f571d7b8b74ebf8c517`）进一步证明，在 2026-04-30 升级发生前，更新服务仍将本机 `LocalVersionBase` 报告为 `8.1.2502.2116`，之后才看到 `ServiceVersionBase=8.1.2604.0917`，并对新版 `ydcc/cemsusbregsiter.dll.zip` 逐文件下载、校验 CRC/大小。这说明 2025 代际是升级前实际运行的 CEMSUsbRegsiter 家族候选，而非单纯历史数据库记录。后续 `audit_ydcc_2025_runtime_fingerprint.py` 已从替换日志恢复旧 `cemsusbregsiter.dll` MD5=`02F8CD326E8CBDA04F17B6235BDF268D`；`audit_current_busmanage_dept_forwarding.py` 又恢复同代被删除的 `usbtoolbusmanage.dll` MD5=`FC29B1C96E48F4F82EA6D641362B3364`，并固定当前 BusManage 的 Dept 转发：请求 `+0x80` 以 `wcsncpy(...,0xBC)` 完整进入写入端 interface，当前 CEMSUsbRegsiter 再把同一字段复制到 `UsbLabelParam+0x40`、容量=`0xBC`。这直接排除当前 BusManage 在写入端前人为制造 Dept[59]=NUL / 59B join；缺失 join59 写入端应继续在旧 CEMSUsbRegsiter 序列化器或同接口旧实现中寻找。由于 2025 两个 DLL 的旧字节仍未恢复，这些证据只收紧 join59/旧版 MBR 写入端的获取目标，不改变 `LBA6+0x03F/+0x1E0..+0x1ED` 与 `LBA9+0x080..+0x0FF` 的部分闭环状态。

#### 真实免密 SanDisk LBA4：盘面 / 读取端 / 写入端三层必须分开

仓库真实免密金标 SHA-256=`d6a935525b9e7bba9926a5ee1e2a74996d2aaf93bc6291723eaaedcff679a258`
的 LBA4，已与原始 `raw/LBA04.bin` 及 concat 对应扇区逐字节比对一致：

- 主 onlyid=`794661040`，滚动后 `OnlyIdXor8` 保护值正确；
- 第二 onlyid=`0x4A32BA39`，HSerial 五 DWORD 非零；
- 节点中 `LLGB`、版本=1、扇区元组=`08 04 0C 01` 正确；
- 物理 `+0x45/+0x46=00 00`，完整滚动读取端视图却为 **`D4 D9`**；
- 保留底层字节 `+0x47..+0x1FB` 并非整段零，不能套原始全零空洞规则。

本轮已把这个分叉进一步闭合到“表示规则已知，并且该实盘的 512B 盘面镜像可由
已取得的官方历史写入端以全零服务器标志精确重建；物理盘当年究竟由哪个
具体可执行文件生成仍不作无证据归因”：

- 当前 Windows `CEMSUsbRegsiter.dll` SHA-256=
  `122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb` 的
  `BuildSector4/sub_10014550`，以及 Linux `BuildSector4@0x1D08E`，都会先对完整
  `+0x18..+0x1FF` 滚动，再把 `node+0x2D/+0x2E` **异或后覆盖**到物理
  `+0x45/+0x46`；
- 更关键的是历史 Windows v19.11.4.1，SHA-256=
  `584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814`，其
  LBA4 编码器 `fcn.10006090` 也在滚动后于 `0x10006249..0x10006257`
  执行相同两次覆盖。对应 SAFE6 `virtual_56@0x1000CC50` 先清零0x2F 节点，
  `0x1000D128..0x1000D15E` 又允许把对象中的5个 HSerial DWORD复制到
  `node+0x08..+0x1B`，而两个标志仍保持初始化值。这直接否定了
  “第二!=主/HSerial!=0 就一定采用普通滚动标志表示”的旧分类器；
- 当前官方读取端 `ReadSector4/sub_10015090` 已由仓库
  `scripts/protocol/probe_lba4_reader.py` 在 Unicorn 隔离内存中直接执行。探测固定
  上述当前 DLL 哈希与真实免密金标哈希，实际覆盖函数 RVA
  `0x15090..0x15295`，只桩函数分配器/MSVC 字符串/`atoi`/安全 Cookie 运行库边界，
  无未映射/异常，返回0并得到 onlyid=`794661040`、正确保护值、
  第二=`0x4A32BA39`、非零 HSerial、`LLGB`、版本=1、元组=`08040c01`、
  `wire_flags=0000`、`reader_flags=d4d9`；
- 新增 `scripts/protocol/probe_lba4_v19_writer.py` 对历史 v19.11.4.1 写入端做
  **内存后端原生执行**：固定 DLL SHA-256 与真实金标 SHA-256，保留样本
  第二=`0x4A32BA39` 与原20B非零 HSerial，仅把恢复节点的两个服务器标志
  设为 `00 00`；调用方负责保留底层字节则按滚动逆变换回写入端输入形态。探测
  只桩函数 PhysicalDrive 路径格式化与调试文本格式化两个 CRT 边界，并把
  `CreateFileA/ReadFile/WriteFile` 重定向到内存；`fcn.10006090@RVA 0x06090` 的
  滚动循环、节点复制、异或后写入全部执行官方机器码。结果 `ret=1`、无
  未映射/异常，唯一一次写入为 `offset=2048,size=512`，输出 SHA-256
  `c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8` 与真实
  SanDisk LBA4 **512/512 完全一致**。这证明 `wire=00 00 / reader=D4 D9` 不需要
  非零写入端标志才能产生；它不声称 v19.11.4.1 就是当年制造该物理盘的精确可执行文件；
- 对 onlyid=`794661040`，滚动 K0=`0xBFED`。在两个标志物理位置上的密钥字节
  分别为 `D4`、`D9`，所以读取端的 `D4 D9` 精确等于 `00^D4, 00^D9`。
  它是**官方读取端视图**，不是已证明的写入端侧标志值。

因此 LBA4 标志的真实规则不能再按 HSerial 世代绑定，而必须按 **写入端
表示家族** 区分：已取得的当前 Windows/Linux 和 v19.11.4.1 写入端
均属于异或后家族；若某盘由这类写入端生成，则盘面字节就是写入端节点
标志，而官方读取端会再 XOR 一次。另一些历史加密实盘则观察到物理非零而
读取端视图为 `00 00`/`0B 00`，与普通滚动表示相容，但其精确
写入端仍未取得；在该家族中读取端视图才对应节点标志。仅凭盘面与身份字段，
目前不能无歧义选择两种家族。

真实免密 SanDisk 的 `00 00 -> D4 D9` 现在不再只是“与异或后家族相容”：上述
v19 虚拟 execution 已证明，在保留其非镜像第二 ID 与非零 HSerial 的条件下，
**写入端节点标志=`00 00` 可以由官方历史写入端精确重建整扇真实盘面镜像**。
但仍不能声称它一定由 v19.11.4.1 生成：已取得的 paired 2020 BusManage 会把 HSerial
请求区清零，仍缺真正给该盘非零 HSerial 的上游调用方；同时也没有该盘
`disk_end-4 sectors` / `disk_end-0x80000` 两份恢复节点镜像的精确捕获。
2026-08-23 原采集只保存 LBA0-LBA13 与 `size-0xE0000` 等其它尾区，不能拿来替代这两个
镜像地址。当前 `RegsiterUsb` 的 LBA4 内部写路径只经过 BuildSector4，
`UnRegsiterUsb` 的逐扇写回序列不含 LBA4；当前修复路径也没有找到只补丁
`+0x45/+0x46` 的证据。另取得历史 `UDiskLabelRepair.dll` 2021-12-08 构建，
SHA-256=`f5e6ddbb4e3097c9968296b43627543ecacdc24b174e52f8f049b289d7264efc`：
`RepairSafe6Label@0x10008B20` 从 `disk_end-0x80000` **整块读取9扇区并原样写回
LBA4-LBA12**，`RewriteSafe6BakLabel@0x100094E0` 反向整块备份；其独立 LBA4 读取端
`fcn.10006DA0` 做完整0xF4 滚动、复制0x2F 节点、只校验 `OnlyIdXor8`。因此这条
历史修复链同样不会凭空制造/清零单独两个标志字节。

实现仍然不猜写入端：`inspect` 的 `decoded` 对 LBA4 始终保持官方滚动读取端
视图，两个标志字段同时展示 `reader=` 与 `wire=`，不会把 `D9` 强行改成0。字段状态则
按写入端/表示/消费端生命周期分别记账：`bDataToServer@+0x045` 仍有
真实 `0B` 读取端配置类型，保持部分闭环；`bConnetServer@+0x046` 的已取得当前
Windows/Linux 与 v19.11.4.1 constructors 都全零初始化且无赋值，历史滚动形式
实盘的正式读取端视图为0，真实免密 SanDisk 又已由 v19 官方写入端以
写入端侧全零精确重建整扇，而所有已审读取端/恢复上层均无该字节的值相关
业务分支。因此 `+0x046` 重新闭合为 **写入端侧未启用全零兼容字节，
完全闭环**。这里完全闭环不等于“读取端总返回0”，也不宣称已知道 SanDisk 当年的
精确制造可执行文件；它只表示该字节的已知写入端值、两类盘面/读取端
表示关系、修复边界和负语义消费端已闭合。该阶段当时的严格统计为
6000 完全闭环 / 656 部分闭环；随后 LBA3 仅原样保留生命周期闭合，当前总计以第3节的
6513 完全闭环 / 143 部分闭环为准。

原采集目录的 `dec/LBA04_dec.bin` 虽显示标志=0，但不能作为独立反证：当时的
`analyze/scripts/read_metadata.py::lba4_decode` 对每一个原始全零字节强制把解码值
归零。同一错误还把物理 `+0x03C=00` 正确解出的 `LLGB` 最后一个 `B` 清成零。
因此本次回归只使用 checked-in 原始金标和正式滚动规则，不依赖历史 dec 文件。

另外，金标中的 Netac `onlyid=949028302 @17:24:33` 与 `@17:23:49` 仍只在 LBA7
不同；第6节已将前者 LBA7 认定为局部实验/中间态。SHA-256去重并不消除该证据限制，
后续清单应显式记录每个 LBA 的生成证据排除范围，不能仅靠整镜像
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
- 虚表 `+0x08` 调用注册；
- 虚表 `+0x0C` 调用 `ChkRegsiterUsb` 校验。

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

### 2.3 CEMSUsbRegsiter：真正的 LBA0–12 写入端

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
  - 其它辅助函数生成剩余标签扇区；
- 最后 `WriteSectorData(..., count=0x0D)` 一次写 **13 个扇区，即 LBA0–12**；
- 写失败日志原源码位置 `usbregsiter.cpp:0x9B3`。

这条调用链是后续字段写入端追踪的根。

### 2.4 官方跨平台源码位置索引

Linux `libcemsfilesyscheck.so` 带 DWARF，可恢复原工程文件和行号。后续字段追踪
优先以这些位置作为写入端/读取端的源码锚点，再用 Windows 当前实现和实盘
交叉验证：

| 区域 | 写入端 | 消费端/读取端 | 原源码位置 |
|---|---|---|---|
| LBA4 | `CLabelManage::BuildSector4` | `CLabelManage::ReadSector4` | `diskfile.cpp:740 / 956` |
| LBA6 | `CLabelManage::BuildSector6` | `CLabelManage::ReadSector6` | `diskfile.cpp:672 / 1005` |
| LBA7 | `CLabelManage::BuildSector7` | LBA7/EDPF 读取端链 | `diskfile.cpp:895` |
| LBA8 | `CLabelManage::BuildSector8` | `CLabelManage::ReadSector8` | `diskfile.cpp:805 / 1102,1143` |
| LBA11 | `CLabelManage::BuildSector11` | `CLabelManage::ReadSector11` | `diskfile.cpp:783 / 1168` |
| LBA12 | `CLabelManage::BuildSector12` | `CLabelManage::ReadSector12` | `diskfile.cpp:921 / 1195` |
| LBA11随机源 | `CDataSecrity::RandBuffer256` | 作为 DataEncrypt/解密 KDF 输入 | `datasecrity.cpp:14` |
| LBA11加/解密 | `CDataSecrity::DataEncrypt` | `CDataSecrity::DataDecrypt` | `datasecrity.cpp:34 / 61` |
| LBA11历史容量兼容辅助函数 | `CDisk::GetWindowsDiskSizeFromLinux` | 当前构建无静态调用方；仅作为兼容函数存在 | `DiskInterface.cpp:1196` |

结构定义的主要 DWARF 源位置：

- `tagEdpPartionInfo`：`global/inc/edpdiskglobal.h:76`；
- `tagNewEdpPartionInfo`：`edpdiskglobal.h:101`；
- `tagEdpPartionPassInfo`：`edpdiskglobal.h:151`；
- `UsbLabelParam`：`diskfile.h:125`；
- `UsbWriteParam`：`diskfile.h:142`。

以上“源码位置”来自 DWARF，本仓库没有复制这些第三方源码；文档只记录定位信息、
反编译证据和伪代码。

### 2.5 LBA6 写入端/读取端伪代码：为什么不能按固定字符串槽粗暴判完成

Linux 写入端：

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

消费端：

`CLabelManage::ReadSector6(char*, UsbLabelParam&) @ diskfile.cpp:1005`

读取端会识别 `0x40245E2A` 溢出标记，并从扩展位置重建长 Dept/User；
GSerial/BeiZhu 则直接按 C 字符串读回。注意 `UsbLabelParam` **没有**
`m_encrypt` 成员；`m_encrypt` 只存在于写入端的 `UsbWriteParam+0x258`。

因此：

- GSerial / BeiZhu 的 **C 字符串语义**可以闭合，但固定 16B 物理槽不能整体计
  完全闭环：写入端固定复制输入对象的前 15B 再补第 16B NUL，消费端只按
  C 字符串读取；输入对象在 NUL 后的保留底层字节字节并没有稳定生成语义；
- Dept/User 虽字段含义明确，但“本槽 + 溢出扩展”必须作为一个整体继续追，
  不能仅看到 `0x000..0x03F` 或 `0x050..0x06F` 就把整槽算完全闭环；
- **LBA6 m_encrypt 当前写入端 `!SAFE` 门禁** 已继续追到 `RegsiterUsb` 的
  当前 Windows 赋值点，而不再只停留在 `BuildSector6` 的落盘动作：
  `sub_100139f0(this+0x2E0 -> temp)` 构造临时 `UsbWriteParam` 后，临时对象基址
  精确为 `ebp-0x3F4`；DWARF 已知 `m_encrypt@UsbWriteParam+0x258`，因此对应
  `ebp-0x19C`。原始 PE 机器码在 `RegsiterUsb@0x1003BA94` 的相等分支把该字节
  写为 `1`，`0x1003BAC7` 的非相等分支写为 `0`；前面的5字节比较目标
  `0x100C9A90` 在 PE 中为 ASCII `!SAFE`。随后 `0x1003BAE2..0x1003BAF5`
  立即把同一临时对象传给 `BuildSector6/sub_10013FD0`，后者再把该字节扩成
  DWORD 写到 `sector6+0x1F0`。Linux `BuildSector6@diskfile.cpp:672` 独立给出
  同一落盘映射。也就是说当前写入端的 1/0 来源已经闭合，不再是
  “22/22 恰好为1”的样本推断。继续核对读取侧后，`UsbLabelParam` 正式结构
  没有 `m_encrypt` 成员；Windows `CheckLabel/sub_100152A0` 在校验前508B
  校验和后会显式解析 Dept/User/GSerial/标签等字段，却不读取
  `+0x1F0`；Linux `ReadSector6` 同样不返回该字段，已扫运行时也无值相关
  消费端。因此这4B可闭合为 **只写 `!SAFE` 标签代际元数据**：
  写入端记录当时的 `!SAFE` 匹配结果，物理字节参与整扇校验和，但不是读取侧控制量。

22份原始盘进一步给出了不能把两个 16B 字符串槽整体标完全闭环的直接反例：

- GSerial：16/22 的 C 字符串为 `"322CA28A"`，6/22 为
  `"322CA28A-D7D144"`；前一组 **16/16 都在 NUL 后仍有非零字节**；
- BeiZhu：20/22 为空、2/22 为 GBK `"普通"`；总计 **8/22 在首个 NUL 后仍有
  非零保留底层字节字节**；
- 两份旧配置类型（Aigo U335、SanDisk）还同时在 `+0x1E0..0x1EF`
  留有非零材料。本轮已证明它不是独立“扩展字段”，而是旧版 MBR
  分区表底层内容的幸存片段：`+0x1DE..0x1ED` 原本是第3条
  16B MBR 条目，BeiZhu 覆盖其前2B 后，`+0x1E0..0x1ED` 仍保留
  CHS/类型/起点/数量；`+0x1EE..0x1EF` 已进入第4条条目。

**LBA6 C 字符串槽具有随配置类型变化的 NUL 后保留底层字节**。因此本轮主动回撤此前对
`+0x1C0..0x1DF` 的过度完全闭环认定。这里是
“字符串含义已知 + 固定槽尾跨写入端配置类型未闭合”的部分闭环，而不是
32B 完整字段。当前写入端的槽尾来自输入对象保留底层字节字节；两份旧版
配置类型则能看到被短 C 字符串局部覆盖后的 MBR 几何残留。

#### m_autoid / Autonum：固定 16B 槽已闭合，包括非语义 NUL 后保留底层字节

写入端：

```text
UsbWriteParam::UsbWriteParam(UsbLabelParam&):
    strcpy_s(writer.m_autoid /* +0x259 */, 16,
             label.m_autoid  /* +0x258 */)

BuildSector6:
    memcpy(sector6 + 0x70, writer.m_autoid, 16)
```

消费端：

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
  **不会清目标缓冲区剩余容量**；同时
  `UsbWriteParam(UsbLabelParam&)@0x1C362` 入口没有整体 memset，
  随后 `BuildSector6` 又固定 `memcpy 16B`，因此 NUL 后内容的写入端
  已闭合为 **写入端未初始化保留底层字节**；
- 已提交原始样本还保留了更强反例：同一个空 autoid 字符串至少存在
  2种不同且非零的 NUL 后保留底层字节，证明这些字节不是第二个隐藏字段。

`ReadSector6` 只解释首个 NUL 前字符串，整扇校验和又保护完整16B物理值。
因此这里现在**增加16B 完全闭环**；完全闭环表示“每个字节的存储/消费行为已知”，
并不表示 NUL 后字节具有固定值。制盘不需要模拟未初始化内存泄漏。

### LBA4 `+0x45/+0x46`：写入端表示与读取端视图分叉

Linux DWARF 给出恢复节点的正式字段名：

```text
tagEdpPartionRestorInfoNode @ edpdiskglobal.h:220, sizeof=0x2F
  +0x2D BYTE bDataToServer
  +0x2E BYTE bConnetServer
```

已取得的三套写入端证据证明 **异或后覆盖并非当前身份专属规则**：

- Windows `cemsusbregsiter.dll::sub_10014550`：先把 0x2F 节点复制到
  LBA4 `+0x18`，对 `+0x18..+0x1FF` 执行完整 0xF4-字滚动 XOR，随后
  明确执行 `LBA4+0x45=node+0x2D`、`LBA4+0x46=node+0x2E`；
- Linux `CLabelManage::BuildSector4@0x1D08E, diskfile.cpp:740`：
  `0x1D278..0x1D2F0` 完成同一 0xF4-字滚动循环后，
  `0x1D302..0x1D329` 再把 `pSerinfo+0x2D/+0x2E` 原样写回
  `buffer+0x45/+0x46`；
- 历史 Windows v19.11.4.1（SHA-256
  `584e591dc679a2dad2c6d15b4ea96f8be39ae40e7841e883ea9cbc2f35a89814`）的
  LBA4 编码器 `fcn.10006090` 先做完整滚动，随后在
  `0x10006249..0x10006257` 从 `node+0x2D/+0x2E` 覆盖到物理标志字节。
  同版本 SAFE6 `virtual_56@0x1000CC50` 的 0x2F 节点先整体全零初始化，随后
  `0x1000D128..0x1000D15E` 可把对象5个 HSerial DWORD复制到节点；两个标志
  没有后续赋值。因此即使第二/HSerial 呈旧版身份，也可以走异或后
  表示。旧检查用身份形态判断盘面规则的前提至此被静态证伪。

读取端则存在一个必须明确记录的非对称行为：

- Windows `sub_10015090` 对 `+0x18..+0x1FF` 统一滚动解码后，直接复制
  `decoded+0x18` 的 0x2F 节点，只校验 `OnlyIdXor8`；没有把物理
  `+0x45/+0x46` 恢复回来；
- Linux `CLabelManage::ReadSector4@0x1E048, diskfile.cpp:956` 同样在
  `0x1E18C..0x1E1D8` 完整滚动，再 `memcpy(decoded+0x18, 0x2F)`，随后仅
  比较 `OnlyIdXor8 == onlyid ^ 0x88888888`；也没有解码后修正。

因此必须按 **写入端表示家族** 区分三层事实：

1. **盘面字节**：真实盘物理 `raw[0x45]/raw[0x46]`，只说明盘面是什么；
2. **官方 ReadSector4 变换后字节**：读取端对物理字节统一执行滚动，
   本身不会补偿异或后写入端的覆盖；
3. **写入端侧节点标志**：只有写入端来源已知时才能判定。对当前
   Windows/Linux 与 v19.11.4.1 异或后家族中，写入端节点字节 == 盘面字节；
   对历史普通滚动兼容实盘，若其缺失写入端确实按普通滚动
   生成，则写入端节点字节 == 读取端视图。身份形态本身不能选择二者。

旧阶段严格22份原始代际参考集的复算仍作为历史观测保留：

- **22/22** 的物理字节与通用滚动解码后字节不相等；
- **6/22 当前身份**：同时满足
  `OnllyID2Nd == main onlyid && HSerialCRC[5] == 0`；物理=`00 00`，
  通用读取端输出为6组不同非零值。该组与异或后写入端家族一致；
- **14/22 旧版身份**：`OnllyID2Nd != main onlyid && HSerialCRC[5] != 0`，
  物理为非零字节，通用读取端输出=`00 00`；
- **2/22 旧版身份特殊配置类型**（Aigo U335 rev_pmap + 独立 SanDisk）：同属
  旧版身份配置类型，物理非零，通用读取端输出=`0B 00`。

新增反例进一步证明 **原始全零/完整滚动物理表示与当前/旧版身份不是同一个维度**：
严格原始 Kingston `2026-08-27 17:30:24` 的恢复节点已是当前身份
（`OnllyID2Nd==main onlyid && HSerialCRC[5]==0`），但 `+0x47..0x1FB` 仍为
物理原始全零；同盘 `17:28:57` 的14扇区只读快照与其逐字节完全一致，排除两次采集
之间临时改写。CI 夹具
`kingston_20260827_current_identity_raw_zero_lba4.bin`（LBA4 单扇区，SHA-256
`85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec`）与
`lba4_raw_zero_short_form_also_exists_in_a_current_identity_profile` 锁定该反例。
因此后续追原始全零历史写入端时，**禁止**再用 OnllyID2Nd/HSerialCRC 代际形态作为
短/完整表示的选择条件；真实选择条件仍未定位。

新增真实免密 SanDisk 又给出决定性反例：第二=`0x4A32BA39`、HSerial非零，
但物理标志=`00 00`，当前官方读取端输出=`D4 D9`。所以此前两个极端模型
以及身份分类器都不成立：既不能把通用滚动结果对所有盘都当
写入端标志，也不能把物理字节对所有盘都当写入端标志，更不能用
第二/HSerial 决定取哪一层。进一步的 v19 写入端探测已保留同一第二/HSerial，
以节点标志=`00 00` 原生执行 `fcn.10006090`，并把该真实 LBA4 **512/512 精确重建**；
所以 `D4 D9` 已被正向证明可以只是异或后写入端遇到非对称读取端后的变换后
字节，而不是必须存在的非零写入端标志。`src/inspect.rs` 仍保持忠实于读取端：

```text
reader = rolling_decode(raw[0x18..])
if historical raw-zero gap:
    reader[0x47..0x1FB] = 0
decoded = reader
flag display = { reader byte, physical wire byte, producer=requires writer provenance }
```

这使 `decoded` 与官方 ReadSector4 行为保持一致，不再让检查替用户猜写入端。
当前/v19 的异或后写入端语义仍保留在审计证据与制盘写入端中，而不是
偷偷改写读取端视图。当前已审 Windows/Linux 上层：

- Windows `ReadRestorInfo/sub_10041290` 只负责读取/重试，不修正这2B；
- `ActiveNormalUDev -> sub_1003CEB0` 不读取 `+0x2D/+0x2E`；
- `GetUpLoadInformation/sub_10039C30` 会把恢复节点暴露给上层，但本 DLL
  内只用身份材料，不读取两个服务器标志；
- Linux `libcemsfilesyscheck.so` 除 BuildSector4 的两次异或后存储外，没有
  对恢复节点 `+0x2D/+0x2E` 的直接字段访问。
- 历史 `UDiskLabelRepair.dll` 2021 构建的 `RepairSafe6Label@0x10008B20` /
  `RewriteSafe6BakLabel@0x100094E0` 只在 LBA4-LBA12 与 `disk_end-0x80000` 间整块复制
  9扇区原始字节；独立读取端 `fcn.10006DA0` 也只是滚动 + 完整节点复制 +
  `OnlyIdXor8` 保护值，没有单字节标志补丁或值相关分支。

这段历史阶段判断已被后续证据取代：`+0x45` 现已由 v19 官方写入端的调用方负责注入实验、严格 Aigo 滚动解码=`0B` 与跨代负向消费端闭合为 **完全闭环**；`+0x46` 则由当前 Windows/Linux + v19.11.4.1 直接写入端全零、
历史滚动形式读取端为零、真实免密物理盘面=`00`、v19 精确 512B 虚拟
写入端 reconstruction、跨代读取端/修复负向消费端一起闭合为
**完全闭环**。其中 `+0x46` 的字段语义是写入端侧未启用全零兼容
字节；官方读取端对异或后盘面可返回非零变换后字节（真实样本即 `D9`），
检查因而绝不能为了完全闭环状态把读取端视图清成0。物理盘当年的精确可执行文件
来源仍未知，但这不再构成该1B生命周期的缺口；它继续构成 HSerial 上游等
其它字段的来源缺口。

同时，当前 SAFE6 制盘已从历史原始全零短表示改为官方
当前写入端的完整表示：完整 `+0x18..+0x1FF` 滚动，然后再
异或后写回 `+0x45/+0x46`。历史原始全零实盘仅作为兼容读取配置类型保留。

### 已验证的远端证据同步（2026-09-21）

本节只收录已经在独立二进制、真实盘或现有回归中核验过、但此前分散在交接/专项
笔记中的证据；它们不会因为“合并文档”而自动增加完全闭环字节。

- **LBA0 / Netac 格式化链**：当前主制标链已静态闭合到
  `BusManageImp::WriteLabelImp -> CCEMSSafeUsbRegsiter::UsbFormat`，但该
  `UsbFormat` 只执行 sectorManage / IIR / 密码兼容预处理，并不调用
  Netac MBR 格式。另一条独立、已验证的兼容层调用链为
  `usb20dll.dll!_IF_DiskFormat -> NewUsb20.dll!FormatExA_NetacAPI`。
  当前安装包仍未找到两条链的连接点，因此历史格式化器选择器继续作为**调用链来源开放问题**保留，禁止因函数名同含 “格式” 就强行拼接。它不再是 LBA0 字节语义阻塞项：20份通用统计集的盘面状态已经被显式全零、UsbMainBSec 与 Netac 三个一方写入端配置类型穷尽。
- **LBA3 / Phison 制造链**：离线静态结果除
  `CBaseController::WriteF2Mark` 外，还确认
  `CU32SSBaseContoller::WriteF2Mark` 与字符串 `F1-F2 MARK`；不同控制器
  类的 F2 暂存长度/包装不同，因此不能把同名 `WriteF2Mark` 当成统一
  512B LBA3 盘面写入端。对严格 Kingston `0951:1666`、62008590336B 的公开
  交叉记录还满足“**公开同身份/容量记录同时存在 PS2307 与 PS2309**”，
  所以控制器型号不能仅凭 VID/PID/型号/容量锁死。
- **LBA4 / 当前节点布局**：已有 **LBA4 当前写入端机器码节点布局**
  证据，当前 `RegsiterUsb` 逐字段把对象成员写入 0x2F 恢复节点；
  本地新增的历史 v19.11.4.1 写入端又进一步闭合 `OnllyID2Nd` 的独立
  `CoCreateGuid -> CRC32_bare` 旧版写入端。
- **LBA6 / 旧版底层内容**：`+0x1E0..+0x1ED` 已按标准 MBR 条目对齐为
  **旧版 MBR 分区表片段**，Aigo/SanDisk 两块真实盘均能与同盘
  LBA12 type4 的类型/起点/数量交叉核对；当前仍缺精确旧版写入端与直接
  消费端，因此保持部分闭环。

## 3. 严格逐字节进度

> 每个 LBA 固定 512B；总计 13 × 512 = 6656B。
>
> 本表的完成率只统计**语义完全闭环**，不把部分闭环计入完成；它不是物理配置类型覆盖百分比。

<!-- STRICT_PROGRESS_BEGIN -->
| LBA | COMPLETE | PARTIAL | UNKNOWN | 严格完成率 |
|---:|---:|---:|---:|---:|
| LBA0 | 512 | 0 | 0 | 100.0% |
| LBA1 | 512 | 0 | 0 | 100.0% |
| LBA2 | 512 | 0 | 0 | 100.0% |
| LBA3 | 512 | 0 | 0 | 100.0% |
| LBA4 | 512 | 0 | 0 | 100.0% |
| LBA5 | 512 | 0 | 0 | 100.0% |
| LBA6 | 512 | 0 | 0 | 100.0% |
| LBA7 | 512 | 0 | 0 | 100.0% |
| LBA8 | 512 | 0 | 0 | 100.0% |
| LBA9 | 512 | 0 | 0 | 100.0% |
| LBA10 | 512 | 0 | 0 | 100.0% |
| LBA11 | 512 | 0 | 0 | 100.0% |
| LBA12 | 512 | 0 | 0 | 100.0% |
<!-- STRICT_PROGRESS_END -->

当前语义总计：

- **COMPLETE：6656B / 6656B = 100.0%**
- **PARTIAL：0B / 6656B = 0.0%**
- **UNKNOWN：0B / 6656B = 0.0%**

物理配置类型正例覆盖不再混入上述百分比。当前 `profile_coverage.tsv` 明确记录：GPT 启用配置类型以及 LBA12 模式1/模式3 尚无 EDP 真实设备正向采集；对应语义由一方运行时正向盘面 + 消费端/算法重放闭合。

LBA11 已完整闭合为 512B 完全闭环。此前卡住的后半 252B 不是“某型号盘偶尔使用
CHS”的未知配置类型，而是来自另一条官方写入端/读取端路径：正常注册写入端使用
`DISK_GEOMETRY_EX.DiskSize`；`UDiskLabelRepair` 的 LBA11 检查与重写使用传统
`DISK_GEOMETRY` 计算 `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector`
作为 `ullSize`。同一 Aigo U335 `rev_pmap` 已同时保留 CHS 与精确 DiskSize 两种真实
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

表中完全闭环行必须同时有写入端、消费端、实盘验证。CI 会解析本表，缺任一列即失败。

<!-- FIELD_LEDGER_BEGIN -->
| LBA | 范围 | 状态 | 字段/区域 | 写入端证据 | 消费端证据 | 实盘验证 | 当前结论 |
|---|---|---|---|---|---|---|---|
| LBA0 | 0x000–0x17A excluding 0x0E1/0x0E8/0x101/0x103/0x10B/0x10D/0x124/0x143/0x162 | COMPLETE | 配置类型级 MBR 引导代码数据块 / 缺失全零配置类型 | 当前 `CUsbRegsiter::RegsiterUsb` 在最终13扇区 `WriteSectorData` 前无条件 `memset(LBA0+0x000,0,0x190)`，形成显式全零缺失配置类型；同一一方 binary 的 `UsbMainBSec@0x100E7220` 提供完整旧版引导代码，且 `UnRegsiterUsb` 有模板直接写回 LBA0 的路径；Netac 配置类型由 `Netac_USB_API.dll` 1.3.1.16（SHA-256 `b12a249a...`）`sub_10003880` 从 `0x1014BA58` 以 `rep movsd, ECX=0x80` 精确复制512B模板再重建分区项。两份静态写入端的前400B已提交为洁净克隆测试夹具 `official_usb_main_bsec_lba0_prefix.hex` / `official_netac_mbr_lba0_prefix.hex`；Netac静态证据登记为 `S-NETAC-MBR`，两份前缀 SHA-256分别为 `4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed` / `00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec` | 两种非零状态都是完整、可自洽的16 位 MBR 引导代码负载：旧版 `UsbMainBSec` 的指令/消息链和 Netac 模板的独立引导代码已静态核对；当前全零配置类型表示该引导代码缺失，EDP 对前400B只按可清除/可替换不透明引导代码处理，不存在另一个值相关 EDP 消费端 | 现行20份通用统计集被严格穷尽为 **8×全零 + 11×UsbMainBSec + 1×Netac**，不存在第四种前400B 盘面状态；真实免密 SanDisk仍属于 UsbMainBSec。两个非零物理前缀均与对应一方静态模板逐字节相等 | 字段语义按配置类型级数据块闭合：每一种已观测盘面状态都有写入端、消费端/缺失行为和物理证据。历史上游格式化器选择器仍是部署/调用链来源的开放问题，但它只选择三个已知写入端状态之一，不再构成这370B的字节语义缺口；未来第四种配置类型必须重新开账 |
| LBA0 | 0x0E1/0x0E8/0x101/0x103/0x10B/0x10D/0x124 | COMPLETE | 七个跨配置类型不变的全零指令操作数字节 | 当前 SAFE6 显式清零全部七字节; 旧版 `UsbMainBSec` 将它们固定为可执行指令中的零操作数：三个 `mov dl,[bp+0]` 位移 (+0x0E1/+0x0E8/+0x124), 三个 `push 0` 立即数 (+0x101/+0x103/+0x10B), 以及 `push 0x7C00` 的低字节 (+0x10D); Aigo/Netac 模板在全部七个偏移处都是全零填充 | 旧版 16 位引导代码会执行相应指令，因此每个零字节都参与已解码操作数; Netac 引导代码的代码/消息在这些偏移之前结束，且没有对这些位置的引用; 当前 EDP 清零且不解析该引导代码 | 已提交的当前全零 + 旧版测试夹具以及专用 Aigo/Netac 前缀证据，在这七个精确偏移处全部为零; 回归测试锁定这些指令字节窗口 | 配置类型选择会改变周边代码，但不能改变这七个物理字节; 它们逐配置类型的写入端/消费端行为已闭环 |
| LBA0 | 0x143/0x162 | COMPLETE | 第一/第二条旧版 MBR 错误消息的 NUL 终止符 / 其他配置类型的全零填充 | 当前 SAFE6 将两字节清零; 旧版 `UsbMainBSec` 在 +0x12C..+0x142 保存 `Invalid partition table`，随后在 +0x143 写 NUL；在 +0x144..+0x161 保存 `Error loading operating system`，随后在 +0x162 写 NUL; Aigo/Netac 模板在这两个偏移处均为全零填充 | 旧版打印循环通过既有消息指针路径把这些 NUL 作为 C 字符串终止符消费; Netac 使用更早位置的消息副本，不引用这些偏移; 当前 EDP 清零后没有引导代码消费端 | 全部已提交当前全零/旧版测试夹具以及专用 Aigo/Netac 前缀证据都保持这两个字节为零; 回归测试检查精确字符串和终止符 | 这 2B 是跨配置类型不变的物理零值；旧版字符串语义和其他配置类型的填充语义均已完整解释 |
| LBA0 | 0x17B | COMPLETE | 第三条旧版 MBR 错误消息的 NUL 终止符 / 其他配置类型的全零填充 | 当前 SAFE6 写入端显式清零到 +0x18F; 旧版 `UsbMainBSec` 在 +0x163..+0x17A 固定保存 `Missing operating system`，随后在 +0x17B 写 NUL; Aigo/Netac 内嵌 MBR 模板在该偏移本来就是零 | 旧版重定位引导代码的打印循环在第三条错误字符串后的 NUL 处终止; Netac 引导代码的三条消息位于 +0x08B/+0x0A3/+0x0C2（最后一个 NUL 在 +0x0DA），因此不会读取 +0x17B; 当前 EDP 清零后没有引导代码消费端 | 已提交当前全零 + 旧版测试夹具以及专用 Aigo L8302 Netac 前缀证据在 +0x17B 都为零 | 配置类型选择不能改变该字节：它在旧版中是消息终止符，在其他已知写入端家族中是全零填充 |
| LBA0 | 0x17C–0x18F | COMPLETE | 跨配置类型固定全零引导代码尾部填充 | 当前 SAFE6 写入端显式清零这 20B; 旧版 `UsbMainBSec` 在第三条消息终止符后立即使用全零填充; Aigo/Netac 内嵌模板在这里同样为零 | 旧版 16 位引导代码在最后一条消息之后没有任何数据/代码引用指向 +0x17C..+0x18F; Netac 引导代码会重定位代码，但最后一条消息在 +0x0DA 结束，没有代码/数据引用指向该尾部; 当前 EDP 不解析该区域 | 已提交当前全零 + 旧版测试夹具以及 `aigo_l8302_netac_lba0_prefix.hex` 都保留 20B 全零; 三个已知物理配置类型逐字节一致 | 写入端所有权、消费端/引用负边界和所有已知真实设备配置类型共同把这 20B 闭环为固定全零引导代码填充; 未来未知配置类型仍必须兼容处理，不能盲目清洗 |
| LBA0 | 0x190–0x19F | COMPLETE | 跨配置类型无所有者原样保留 / 历史全零兼容区域 | 当前 SAFE6 `RegsiterUsb` 最终只清 `+0x000..+0x18F`，因此本16B保持读取前保留底层字节；Linux `BuildSector0@diskfile.cpp:625` 只重建 `+0x1BE` MBR 条目，不写本区；旧版 `UsbMainBSec` 本16B固定为零；Netac `sub_10003880` 整扇复制的 Aigo 写入端模板在本16B同样为零 | 16 位 `UsbMainBSec` 引导代码的直接数据引用落在 `+0x1B5/+0x1B6/+0x1B7` 和分区表/签名，不读取本区；当前注册/准入与 `UDiskLabelRepair::ReCreate0Sector/sub_10003960` 均不解释本16B，修复新建只清前0x190和分区表，保持本区无所有者 | 严格22份原始参考本16B 22/22 全零；扩展 `nopwd_tool/backup + utils/backup` 共57份完整历史快照也 57/57 全零；既有 `lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix` 门禁锁定已提交原始样本 | 完全闭环表示跨已知配置类型的**无业务负载 / 原样保留现有**生命周期闭合，而不是规定未来盘面必须为零；遇到未知非零值应兼容保留 |
| LBA0 | 0x1A0–0x1A3 | COMPLETE | 可选 SAFE1 / 旧版 `SectorSize` 兼容覆盖项 | Windows `BuildSector0/sub_10013F10@0x10013FB6` 在 **SAFE1** 分支明确把 `m_nSectorSize` 写到 `+0x1A0`；`UsbMainBSec` 完整模板同样携带512。Aigo/Netac `sub_10003880` 的整扇 MBR 模板在该槽显式为0；当前 SAFE6 不调用 `BuildSector0` 而原样保留现有保留底层字节，Linux `BuildSector0@diskfile.cpp:625` 只用 `m_nSectorSize` 计算分区扇区数量、不序列化该槽 | 旧版 16 位引导代码不读取该槽；当前 `CEMSUsbRegsiter.dll` 全模块对 `+0x1A0` 的唯一扇区数据访问就是上述写入端存储，另一个 `push 0x1A0` 只是临时 `memset` 长度；`UDiskLabelRepair` 没有盘面 `+0x1A0` 读点，Linux 没有 `ReadSector0` 消费端；`EdpEDiskCtrl` 的 `+0x1A0` 命中已逐项确认是虚表/对象偏移而非 LBA0 数据 | 严格22份原始盘仅0/512双态；已提交测试夹具同时保留0和512。扩展只读历史扫描中，47份相同旧版引导代码快照分为10×0、37×512；把 SectorSize、磁盘签名、分区表三个独立变量归一化后47/47整个LBA0 SHA-256均为 `2c8877b90c5d42d73f17c511ef5984efc8135543bda0746ef54f347320d78e8f`，证明该DWORD只是独立可选覆盖项 | 512表示 SAFE1/旧版写入端填充的扇区大小兼容元数据；0表示该覆盖项缺失/无所有者，后续 SAFE6 只透明保留。写入端/原样保留、负向消费端、0/512 双真实配置类型与物理独立性均闭合，4B 升完全闭环；不要求未来未知值被清洗 |
| LBA0 | 0x1A4–0x1B4 | COMPLETE | 跨配置类型无所有者原样保留 / 历史全零兼容区域 | 与 `+0x190..+0x19F` 相同：当前 SAFE6 不覆盖、Linux BuildSector0 不写；旧版 `UsbMainBSec` 与 Aigo/Netac整扇模板均在本17B生成零 | 16 位 MBR 引导代码不读取本区；当前 EDP 准入、分区修复与 `ReCreate0Sector` 都只处理其它明确区域，不赋予本17B语义 | 严格22份 22/22 全零；扩展57份完整历史快照 57/57 全零；已提交原始门禁已有精确零断言 | 17B 的跨配置类型无所有者/原样保留、负向消费端与真实盘已闭合；未来非零兼容值必须原样保留，不得机械清零 |
| LBA0 | 0x1B5–0x1B7 | COMPLETE | **LBA0 旧版 MBR 消息指针字节** | 官方 `UsbMainBSec@0x100E7220` 固定为 `2C 44 63`；`sub_10013FD0`/旧模板写路径整扇复制该模板 | 模板先把 `+0x1B..` 搬到 `0x061B` 后执行；运行时 `mov al,[0x07B5/0x07B6/0x07B7]` 分别组成 `SI=0x072C/0x0744/0x0763`，指向原模板 `+0x12C/+0x144/+0x163` 三条错误消息 | 22盘严格统计：14/22=`2C 44 63`，8/22=`00 00 00`，无第三种值；CI夹具同时保留两种配置类型 | 三字节是旧版 MBR 错误消息指针低字节；零态表示该旧版尾部未存在/已清空，不再当“未知随机尾巴” |
| LBA0 | 0x1B8–0x1BB | COMPLETE | 标准 Windows MBR 磁盘签名 | `CreateDiskMbr` 取 `GetSystemTimePreciseAsFileTime`（回退 `GetSystemTimeAsFileTime`）→ FILETIME 转 Unix 秒 → 低32位填 `CREATE_DISK_MBR.Signature` → `IOCTL_DISK_CREATE_DISK`；Windows `DRIVE_LAYOUT_INFORMATION_MBR.Signature` 正式定义该 DWORD 为唯一标识 MBR 磁盘的驱动器签名 | Windows `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 将该值作为 `DRIVE_LAYOUT_INFORMATION_EX.Mbr.Signature` 返回。当前 `CEMSUsbRegsiter.dll::fcn.10046320` 的唯一 `0x70050` 调用已逐指令复核：返回缓冲区只比较 `+0x04 PartitionCount==1` 与 `+0x00 PartitionStyle==MBR`，不读取 `+0x08 Mbr.Signature`，证明 EDP 对该标准字段没有附加业务语义 | 22/22非零，19个值；同一 onlyid 的重复备份保持不变，按LE解释与历史初始化日期吻合；筛选后的协议测试夹具又锁定签名非零且至少存在两个不同真实值 | 写入端、Windows 标准消费端/字段语义、EDP 负向语义消费端与真实盘变化均闭合；4B 升完全闭环，不能把“EDP 不读取”误当成字段语义未知 |
| LBA0 | 0x1BC–0x1BD | COMPLETE | 标准 MBR 保留 / 无所有者兼容字 | 当前 SAFE6 `RegsiterUsb` 只清 `+0x000..+0x18F` 并在 `+0x1BE` 起重建分区表，因此这2B保持读取前保留底层字节；Linux `BuildSector0@diskfile.cpp:625` 同样不写本槽；旧版 `UsbMainBSec` 与 Aigo/Netac 整扇模板都在此显式携带 `00 00` | 16 位 `UsbMainBSec` 引导代码的已确认尾部引用不包含 `+0x1BC/+0x1BD`；当前注册/准入、`UDiskLabelRepair::ReCreate0Sector` 与已审 `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 路径均不把这2B作为业务字段读取，分区语义从 `+0x1BE` 开始 | 严格22份原始参考 22/22=`00 00`；扩展 `nopwd_tool/backup + utils/backup` 的57份完整历史快照同样 57/57=`00 00`，且相邻磁盘签名明确多值，排除“整段尾部碰巧固定”的误判 | 2B 的跨已知配置类型写入端/原样保留生命周期、负语义消费端与实盘均闭合；完全闭环表示该保留字当前无业务负载，未来未知非零值应兼容原样保留，不得机械清零 |
| LBA0 | 0x1BE–0x1FD | COMPLETE | 4×MBR 分区条目 | `UsbMainBSec` 模板；SAPF 恢复项也直接写回此处 | `UDiskLabelRepair.dll::Repair0Sector` 直接恢复该 64B 区域 | 22/22 可按标准 MBR 解码 | 分区表边界和消费闭合 |
| LBA0 | 0x1FE–0x1FF | COMPLETE | MBR 55AA | 官方模板直接写 `55 AA` | MBR 校验/修复链检查签名 | 22/22 | 完成 |
| LBA1 | 0x000–0x1FF | COMPLETE | 可选 GPT 主头 / 缺失配置类型扇区 | Linux官方 `CLabelManage::BuildSector1_Gpt@0x1FDAA` 原生构造完整512B `GPT_Header`：模板先完整覆盖512B，动态写备份/last-usable/磁盘 GUID/表 CRC/头 CRC；`+0x5C..+0x1FF` 由模板明确为零 | Windows 当前 `IsAllowRegisterCommonLabel/sub_1002AB70` 在 protective MBR 命中后，以 sector_size 跳到 LBA1 并检查 `EFI PART` 与 `header_lba==1`；本轮把官方 Linux 构造器输出直接喂给该 Windows 消费端，原生返回 `2=GPT` | 22/22 物理原始样本与扩展3896候选均为缺失 GPT 全零配置类型；另新增 **官方二进制虚拟写入端** 正向测试夹具：2GiB/512B配置下 `EFI PART`, 版本=0x10000, 头大小=92, 备份=4194303, 第一/last usable=34/4194270, 表 LBA=2, 128×128B；独立 IEEE CRC32 同时命中头 CRC `0xA4B46C72` 与16KiB 数组 CRC `0xD32CFEA7`；CI锁定 | 两种生命周期均闭合：非GPT时 保护性 MBR 门禁不进入LBA1语义，当前样本为零；GPT时一方写入端完整拥有512B并被独立 Windows 一方消费端正向识别。虚拟测试夹具不冒充物理采集 |
| LBA2 | 0x000–0x07F | COMPLETE | GPT 分区条目0 | Linux官方 `BuildSector2_Gpt@0x1FFF6` 每次完整写一个128B `GPT_Partition`：Basic Data 类型 GUID固定；分区 GUID为调用方输入；起点固定63；终点=`63+caller_size`；属性/名称来自零模板 | Windows `sub_1002B2F0` 与 Linux `AnalyzeGptPartitionTable` 均按128B 步长比较类型 GUID，命中后读取 `start@+0x20/end@+0x28/attr@+0x30`；GPT 头又规定条目大小=128 | 官方二进制虚拟写入端以 GUID `001122...eeff`、2GiB容量直接生成条目0，得到起点=63/终点=4194270/属性=0/名称全零；同一输出参与 LBA1 分区数组 CRC 并被 Windows GPT 头消费端接受 | 128B 写入端→盘面→消费端字段闭合；分区 GUID 16B 为显式调用方负责身份材料，名称/属性为模板零。该正例与物理统计集分层 |
| LBA2 | 0x080–0x08F / 0x100–0x10F / 0x180–0x18F | COMPLETE | GPT 条目1..3 的未使用 `PartitionTypeGUID` | 当前 Windows `WriteNormalULabel` 大盘分支调用 GPT 构造器 `sub_10037160(..., partition_count=1)`；该函数向 `IOCTL_DISK_CREATE_DISK` 传 `PartitionStyle=GPT(1)`，并向 `IOCTL_DISK_SET_DRIVE_LAYOUT_EX` 提交 `DRIVE_LAYOUT_INFORMATION_EX.PartitionCount=1`。因此条目0 后三个槽在当前一分区配置类型中均为未使用 GPT 条目；UEFI 2.10 §5.3.3 定义未使用条目的 `PartitionTypeGUID=00000000-0000-0000-0000-000000000000` | Windows/Linux GPT 解析器都以16B 类型 GUID 判定条目是否有效；类型 GUID 为0时该条目不进入起点/终点/属性语义解析 | 官方 Linux 虚拟 GPT 测试夹具的条目1..3 三个类型 GUID 均为0；另对本机20,538个候选文件只读扫描得到1份真实 GPT 镜像（Ubuntu 26.04 ISO），其3个已用条目后至少125个未使用条目的类型 GUID/完整条目均为0。物理 EDP 原始样本仍为缺失 GPT 全零配置类型 | 这里只升级每条未使用条目的16B 类型判别项。其余112B仍不借助测试框架预清或通用规范推断；三条共48B从部分闭环升完全闭环 |
| LBA2 | 0x090–0x0FF / 0x110–0x17F / 0x190–0x1FF | COMPLETE | **GPT 条目1..3 未使用条目无所有者残留** | 当前 Windows GPT 构造器明确只提交 `PartitionCount=1`，因此条目1..3 的 `PartitionTypeGUID` 为零时整条条目已处于未使用状态；这336B不是 EDP 自定义负载，Windows kernel 是否把残留具体初始化为零不影响其协议语义。Linux `BuildSector2_Gpt` 只负责有效条目，同样不赋予未使用残留独立字段语义 | 当前 Windows `CPartitionType::AnalyzeGptPartitionTable/sub_1002B2F0` 的机器码先比较每条条目的16B TypeGUID；仅在匹配受支持非零 GUID 后才读取 `+0x20/+0x28/+0x30` 等残留字段。Linux `AnalyzeGptPartitionTable@0xFB36` 独立同构：`memcmp(type_guid, entry,16)` 命中后才读起点/终点/属性。隔离 Unicorn 进一步让 Windows 官方构造器原生建立支持的 GUID 映射（仅映射 SEH 零页并桩函数 `HeapAlloc/HeapFree` CRT 边界），再喂4条 `TypeGUID=0 + residual=0xA5` 的条目；解析器 `ret=0`、无异常，内存读取钩子对336B 残留 **0次读取**，四条条目均在 TypeGUID 起始比较即短路 | Linux 一方虚拟 GPT 测试夹具与独立 Ubuntu GPT实盘中的未使用残留均为零；新增 Windows 一方消费端探测又证明任意非零残留不进入语义消费。这里不把 `0xA5` 探测冒充写入端输出，而是用于证明“未使用后残留值无业务意义” | 按与 LBA4/LBA5 无所有者保留底层字节一致的完全闭环口径闭合：决定条目是否存在的是16B TypeGUID；为零后其余112B/条目属于无所有者残留，兼容读取不得赋予隐藏语义或强制依赖零值。至此 LBA2 512/512 完全闭环 |
| LBA3 | 0x000–0x1FF | COMPLETE | **制造商负责不透明 MP 元数据 / EDP 仅原样保留扇区；不得与 MPALL F2 信息页直接等同** | 当前 Windows `CUsbRegsiter::RegsiterUsb` 先读完整13扇区，SAFE6注册链没有 LBA3 构造器，最终把同一暂存镜像整段写回，因此 LBA3 的 EDP 写入端语义是原样保留现有；Linux 独立不存在 `BuildSector3`。历史 v19.11.4.1 又由 `scripts/protocol/audit_v19_lba3_preserve.py` 固定 SHA-256 后枚举全 DLL 31 个 `SetFilePointer`，可恢复的固定扇区大小倍率精确为 `{1,2,4,6,7,8,12}`、无3，SAFE6 `virtual_56@0x1000CC50` 也不直接调用通用 绝对寻址包装函数 | 当前 Windows/Linux 注册、登录路径没有 LBA3 负载解析器；2021 `UDiskLabelRepair.dll` 的 `RepairSafe6Label/RewriteSafe6BakLabel` 只在 LBA4-LBA12 与尾部镜像之间整块复制9扇区，天然绕过 LBA3。Phison `WriteF2Mark/GetInfo` 分析继续作为边界反证：F2 信息不是主机 LBA3，不能把厂商内部位强行赋予 EDP 语义 | 22份原始参考21/22全零、1份严格 Kingston 非零；扩展历史另有第二种不同非零 MP 配置类型。两种非零配置类型均保留 `+0x001=01` 与 `+0x1F0..1FF="this is mp mark\0"`，但 `+0x020..027` 不同，证明 EDP 必须透明保留而不能零填或套固定模板 | **EDP协议层生命周期闭合**：这512B属于外部制造商拥有的不透明元数据，EDP 的逐字节规则是原样保留/忽略。精确 Phison 主机序列化器、控制器/固件私有字段含义继续作为制造商来源研究问题，不再构成 EDP LBA0-LBA12 字节语义阻塞项；未来任意未知非零 LBA3 都必须原样保留 |
| LBA4 | 0x000–0x017 | COMPLETE | `$$$onlyid$$$` 清零头 | 当前注册写入端根据主 onlyid 格式化 | 识别/解码链从此恢复 onlyid | 22/22 | 完成 |
| LBA4 | 0x018–0x01B | COMPLETE | OnlyIdXor8 | 当前写入端: `main_onlyid ^ 0x88888888` | 恢复信息读取该字段 | 22盘当前/旧版可解 | 完成 |
| LBA4 | 0x01C–0x01F | COMPLETE | `OnllyID2Nd` = 备份/激活加密密钥种子 | **当前写入端**：Windows `RegsiterUsb@0x1003BBD1..0x1003BBD7` 直接执行 `node+0x04 = object+0x698`，即复用本次注册主 onlyid。**旧版写入端**现已从官方归档 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5 `783d01f19e998a514834bc5e5f4249ad`）恢复：SAFE6 `virtual_56@0x1000CC50` 清零0x2F 恢复节点后调用 `fcn.100058E0`；该函数 `CoCreateGuid()` 生成16B GUID，初始化协议 CRC 表，以初值0逐字节计算同款反射形式 `CRC32_bare`，返回DWORD；`0x1000D122` 将结果精确写入 `node+0x04`。因此早期配置类型是“**独立 GUID-CRC 密钥**”，当前配置类型改为“**复用主 GUID-CRC onlyid**” | 有效消费端已闭合为完整往返验证：`GetUpLoadInformation/sub_10039C30` 以 `restore_node+0x04` 为4B 种子加密 LLGB+EDPF 备份数据块；`ActiveNormalUDev -> sub_1003CEB0` 取同一种子解密激活/恢复数据块，校验 `LLGB` 后恢复 LBA8/LBA12。两辅助函数的16B 密钥材料均为 `key4[i mod 4] XOR "EDPSECDISK200709"[i]`，随后走同一密钥序列、互为加密/解密块变换 | 严格当前配置类型 `OnllyID2Nd==main onlyid && HSerialCRC=0`；旧版配置类型 `OnllyID2Nd!=main`。可复核旧版第二密钥不等于本设备/全集主 onlyid；已提交 NETAC_A/NETAC_B/LEXAR 精确锁定 `44D9CE02/028EFFD3/7647B1EF`，并新增门禁证明它们也不退化为 `CRC32(device_id)`、MBR 磁盘签名或 `MyHardinfo`；同一主的重复捕获第二密钥保持稳定，符合“制标时生成后持久化”的随机密钥生命周期 | 4B 的正式边界、当前/旧版双写入端、随机生成算法、双向密码学消费端和真实配置类型均闭合；不同代际只改变种子来源，不改变备份/激活密钥语义，升级完全闭环 |
| LBA4 | 0x020–0x033 | COMPLETE | 调用方负责 `HSerialCRC[5]` / `HDOnlySerial[5]` 身份向量 | 当前 Windows PE `RegsiterUsb@0x1003BBF0..0x1003BC79` 精确把 `object+0x558/+55C/+560/+564/+568` 写入 `node+0x08..+0x1B`；`this+0x2E0` 已闭合为内嵌 `UsbLabelParam`，故这些地址正是 `HDOnlySerial[5]@+0x278`。当前 Windows 两层参数构造以及 Linux 调用方均不提供这20B，所以当前配置类型为全零/缺失。历史 v19.11.4.1 `ISUdiskRegsiterObj::virtual_8@0x1000B9C0` 则把旧版 ABI `request+0x150..+0x160` 五个DWORD逐项原样复制到对象 HSerial 槽，SAFE6 再原样放入恢复节点。`scripts/protocol/probe_lba4_v19_writer.py` 固定 v19 DLL/金标 SHA，保留真实免密 SanDisk 的真实非零20B HSerial 输入，原生执行 `fcn.10006090` 后输出 LBA4 SHA-256=`c2662856...`，512/512与物理金标完全一致 | **历史恢复节点读取端/激活消费端** 已闭合为结构性保留 / 语义忽略：2020 `ActiveNormalUDev` 经虚表 `+0x2C` 进入 `ReadUsbHserialsInfo@0x100054A0`，依次从 LBA4、disk_end-4 扇区、disk_end-0x80000 三个镜像读取并滚动解码完整0x2F 节点；三套读取端只比较 `OnlyIdXor8`。随后虚表 `+0x44` `RestoreRegsiterUsb` 只取 `node+0x04 OnllyID2Nd` 作为恢复数据块密钥，不读取 `+0x08..+0x1B` 的五DWORD值。`EDP_DeviceNumber/EDP_DiskNumber` 是读取端成功后另行返回的单DWORD 标量，ABI直接排除它与HSerial五槽等价 | 严格22份覆盖14份固定 `1D29,7B,4DD,79,7C`、6份全零、2份高熵，且真实免密 SanDisk 的非零20B已由一方 v19 写入端在不修改该向量的条件下逐比特精确重建整扇 | 字段级生命周期按调用方负责身份向量闭合：正式字段边界、当前缺失全零配置类型、旧版调用方注入 ABI、官方写入端传输、三镜像读取端、负语义消费端和多种真实非零配置类型均已确定。更早调用方为什么/如何计算五个DWORD仍是值的代际来源开放问题，类似其它调用方负责身份材料，不再构成这20B盘面语义缺口；不得把 DeviceNumber/DiskNumber 猜成该算法 |
| LBA4 | 0x034 | COMPLETE | 固定恢复节点 `SingleUsbFlg` 元数据 = 0 | 当前 Windows 恢复节点写入端清零节点后显式保持/写入 `SingleUsbFlg=0`；Linux 官方节点 ABI确认该字节的结构位置 | Windows/Linux `ReadSector4` 都对完整0x2F 恢复节点做滚动解码后结构性返回；除 `OnlyIdXor8` 外不对该字节做值相关分支，因此是结构性保留 / 语义忽略元数据 | 已提交原始测试夹具全量门禁 + 22份严格原始样本均为0，跨当前/旧版身份配置类型无反例 | 写入端、正式字段边界、结构消费端、负语义消费端与跨代实盘一致，1B 完全闭环；不是运行时开关 |
| LBA4 | 0x035–0x038 | COMPLETE | `MyHardinfo` = 已观察镜像于 LBA8 `HDSerialInfo` | 当前 SAFE6 `RegsiterUsb` 对完整0x2F 节点清零且不覆盖 `node+0x1D..20`，因此当前写入端=0。历史官方 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5 `783d01f19e998a514834bc5e5f4249ad`）SAFE6 `virtual_56` 在 `0x1000D189` 调 `UsbTools.dll` ordinal4=`EDP_DiskNumber`，返回0才回退 ordinal3=`EDP_DeviceNumber`，并在 `0x1000D19B` 把结果直接写到 `node+0x1D MyHardinfo`；同一二进制 LBA8 写入端 `sub_10007DF0@0x10007E82..` 独立调用同一 ordinal4/3 并把结果写入 `HDSerialInfo`，从写入端机制解释跨LBA镜像 | 历史三套LBA4 读取端完整结构返回节点但只比较OnlyIdXor8；2020 `ReadUsbHserialsInfo -> RestoreRegsiterUsb` 链又证明恢复端只消费 `node+0x04`，不读取 `MyHardinfo`。因此 LBA4 副本是结构性保留 / 语义忽略兼容元数据 | 严格原始样本逐盘 **22/22 `MyHardinfo == LBA8.HDSerialInfo`**，同时保留当前 `0->0` 和旧版非零 `A017AD78/A68BAE08/8B4613F5/2AB0E33C`；该值不等于设备 ID CRC或MBR 签名 | 字段级生命周期现已闭合：当前全零与旧版主机身份写入端家族、LBA4/LBA8 独立镜像写点、22/22 物理镜像、跨不同目标U盘复用同一主机身份信息 值以及负语义消费端共同限定了本 DWORD。v19 的相邻 `UsbOnlyInfo` 与严格旧版不同只证明另一个16B字段存在代际分叉，不再作为本字段阻塞项；精确制造可执行文件未定位不影响该4B含义闭环 |
| LBA4 | 0x039–0x03C | COMPLETE | 固定恢复节点 `NewLabFlag = LLGB` | 当前 Windows 机器码在节点清零后显式写 `LLGB`；Linux DWARF恢复正式字段与偏移 | Windows/Linux 读取端解码并结构性返回完整节点，不对该字段做独立行为判断 | 已提交原始测试夹具全量门禁 + 严格 22/22 均为 `LLGB`，跨当前/旧版配置类型一致 | 固定写入端元数据 + 结构性保留/语义忽略 + 真实设备配置类型闭合，4B 完全闭环 |
| LBA4 | 0x03D–0x040 | COMPLETE | 固定恢复节点 `Version = 1` | 当前 Windows 写入端显式写 DWORD 1 到恢复节点版本；Linux ABI给出字段边界 | Windows/Linux 读取端把版本随完整节点返回，当前没有值相关准入/行为分支 | 已提交原始测试夹具全量门禁 + 严格 22/22 均为1 | 当前已知协议代际中的固定恢复节点版本元数据生命周期闭合，4B 完全闭环；未来新版本非1时应按新配置类型处理而非强制改写 |
| LBA4 | 0x041–0x044 | COMPLETE | 固定恢复节点扇区元组 `08 04 0C 01` | 当前 Windows 写入端在节点构造阶段显式写四个扇区字节 `08 04 0C 01` | Windows/Linux 读取端随完整恢复节点结构返回这些字节，但当前没有独立值相关分支 | 已提交原始测试夹具全量门禁 + 严格 22/22 均为 `08 04 0C 01`，跨当前/旧版配置类型一致 | 写入端、结构边界、负语义消费端与真实盘全部闭合，4B 完全闭环；按固定兼容元数据建模 |
| LBA4 | 0x045 | COMPLETE | 调用方负责 `bDataToServer` 兼容字节；盘面表示随写入端家族变化 | 当前 Windows/Linux 与历史 v19.11.4.1 `fcn.10006090` 都把完整0x2F 节点作为输入；v19 写入端在完整滚动后执行 `node+0x2D -> wire+0x45` 异或后存储。扩展后的 `probe_lba4_v19_writer.py --node-flags 0b00` 原生执行官方机器码，除 `+0x45: 00->0B` 外 512B 无任何变化，证明该字节是透明调用方负责输入而非写入端内部派生 | Windows/Linux ReadSector4 滚动后结构性返回完整节点；2021 修复读取端同样完整复制节点且只校验OnlyIdXor8。ActiveNormalUDev/RestoreRegsiterUsb只消费OnllyID2Nd，GetUpLoadInformation虽携带节点但本DLL没有 `+0x2D` 值相关读取；Linux DWARF中该节点只进入BuildSector4/ReadSector4。因此已覆盖消费端对该字节是结构性保留 / 语义忽略 | 严格 Aigo U335 `onlyid=1987718388` 的物理标志=`64 7A`；按正式滚动精确得到逻辑节点标志=`0B 00`，其中对应密钥字节为 `6F/7A`，即 `64^6F=0B`、`7A^7A=00`。同盘免密转换前后LBA4 512/512不变，排除转换工具生成该值；其它严格/异或后配置类型与真实 SanDisk 又覆盖全零/非零读取端视图 | 字段生命周期按调用方负责兼容元数据闭合：逻辑值由上游调用者选择，写入端只透明序列化，读取端/恢复无隐藏派生或值相关语义。更早普通滚动制造可执行文件未取得只影响盘面表示来源，不再构成本1B盘面语义缺口；不得把物理盘面字节直接当逻辑标志 |
| LBA4 | 0x046 | COMPLETE | `bConnetServer` 写入端侧未启用全零兼容标志；读取端视图随盘面表示可非零 | 当前 Windows/Linux 与 v19.11.4.1 写入端都在滚动后异或后覆盖节点+0x2E；这些 SAFE6 节点构造器均全零初始化且无后续标志存储，所以直接写入端侧=0。新增 `scripts/protocol/probe_lba4_v19_writer.py` 保留真实免密 SanDisk 的第二=`0x4A32BA39`/非零 HSerial，只将节点标志设为`00 00`，内存后端原生执行 v19 `fcn.10006090` 后唯一一次 LBA4 写入与真实扇区512/512完全一致，SHA-256=`c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8` | Windows/Linux ReadSector4统一滚动并返回该字节而不修正；当前读取端探测对真实免密盘得到`D9`，证明读取端视图不是写入端值。2021 历史 `UDiskLabelRepair::fcn.10006DA0` 独立执行同款滚动、完整复制0x2F 节点且只校验OnlyIdXor8；其修复/备份路径整块复制9扇区，不单独修改标志；ActiveNormalUDev/GetUpLoadInformation等已审上层无该字节值相关业务分支 | 严格加密历史滚动形式样本的正式读取端视图本字节为0；真实免密金标 SHA-256 `d6a935...` 为物理盘面=`00`、读取端=`D9`，且 v19 官方写入端以写入端节点字节=0 精确重建整扇 | 生命周期按“写入端侧全零 + 表示相关 读取端变换”闭合；完全闭环不意味着读取端必为0，也不声称 v19 是该物理盘当年的精确制造可执行文件。精确来源仍属于 HSerial 上游等其它字段的调查范围，不再构成本1B语义缺口 |
| LBA4 | 0x047–0x1FB | COMPLETE | **无所有者保留底层字节 / 表示载体**；原始原样保留与滚动变换后两种盘面表示 | Windows 当前 `sub_10014550` 与 Linux `BuildSector4@diskfile.cpp:741` 都只拥有0x2F 恢复节点。非空节点分支随后把滚动 XOR 覆盖到 `+0x18..+0x1FF`，因此对这437B只是**可逆变换既有保留底层字节**；Windows `arg0==NULL` 分支则完全跳过节点复制/滚动，对该区逐字节原样保留。两端都没有独立业务字段存储。隔离 Unicorn 直接执行官方 Windows 写入端：预填437B=`0xA5` 时，完整分支原始字节改变但独立滚动解码后437/437恢复 `0xA5`；NULL 分支437/437保持原始 `0xA5` | Windows `ReadSector4/sub_10015090` 与 Linux `ReadSector4@diskfile.cpp:957` 都会为恢复节点识别需要而滚动处理整段，但最终只返回 `decoded+0x18` 的0x2F 节点并校验 `OnlyIdXor8`，不暴露/解释 `+0x47..+0x1FB`。同一官方 Windows 读取端对上述非零完整测试夹具动态执行成功，返回主 onlyid、LLGB、版本=1，而437B不进入输出 | 严格 22份仍保留18份原始全零与4份滚动全零物理表示；新增一方虚拟写入端测试夹具又证明保留底层字节可合法为任意非零值并在完整/null 两分支分别“变换/原样保留”。CI `official_virtual_lba4_backing_is_unowned_and_representation_only` 锁定非零正例 | 437B 的含义不是“应该为零但最初写入端未找到”，而是**没有业务负载的调用方/现有保留底层字节**。其完整生命周期已由原样保留/可逆变换写入端、负语义消费端、真实双表示和任意非零一方正例闭合；与 LBA5 不透明原样保留区采用同一完全闭环口径。历史原始全零最初来源不再是语义阻塞项，未来未知非零保留底层字节必须保留/变换而不得清洗 |
| LBA4 | 0x1FC–0x1FF | COMPLETE | 尾部 LLGB | 当前写入端继续滚动密钥序列写 LLGB | 读取端作为尾锚点校验 | 22盘可验证 | 完成 |
| LBA5 | 0x000–0x1FF | COMPLETE | 不透明原样保留 / 写保护探测临时扇区 | `CUsbRegsiter::RegsiterUsb` 先读取既有 LBA0–12；后续构造器只重建其它明确扇区，LBA5 不被覆盖，最终随13扇区整体写回；即写入端语义是原样保留现有字节 | 两版 `EdpDiskCtrl` 的唯一 `base+5` 原始扇区消费端都是：读取整扇→原样写回同一扇区→仅检查 `WriteFile` 是否以 `ERROR_WRITE_PROTECT(0x13)` 失败；`UserLogin` 据此进入只读使用状态，完全不解析内容 | 22/22原始参考整扇512B全零，SHA-256均为 `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`；7份原始CI夹具继续锁定 | 完全闭环表示“整区用途和无负载语义闭合”；全零只是当前实盘状态，不是协议规定，非零内容也应原样保留 |
| LBA6 | 0x000–0x03E | COMPLETE | Dept 主槽前63B：短 C 字符串/保留底层字节或长标记 + Dept[0..58] | Linux `UsbWriteParam(UsbLabelParam&)@0x1C362` 对部门调 `strcpy_s(dst+0x40,0xBC,src+0x40)`；复制构造器不预清对象且自带 `strcpy_s@0x1B9B0` 复制到NUL即停，所以短配置类型的 NUL 后字节是写入端未初始化保留底层字节。Linux `BuildSector6@0x1CAAC` 在 `strlen<=63` 时固定 memcpy 完整64B；在长配置类型时先清64B临时槽，写标记 `0x40245E2A`，再把 Dept 前60B 放到标记后，其中 `+0x04..+0x3E` 正好是 Dept[0..58] | Windows/Linux `ReadSector6`：短配置类型按 C 字符串读取；长配置类型识别标记后把内联前缀与 LBA9+0x80 续段重组。当前/旧版两种长读取端均共享标记 + Dept[0..58] 这63B，接缝差异只发生在最后1B `+0x3F` | 已提交原始样本的短配置类型至少3份，LBA6 Dept C 字符串与 LBA8 `Dept=` 一致，且在 `+0x00..+0x3E` 内保留真实 NUL 后非零保留底层字节。严格原始当前 Kingston join60 与旧版 Lexar join59 的解密 LBA6 `+0x00..+0x3E` **63B逐字节完全相同**；回归 `lba9_dept_continuation_preserves_both_official_reader_join_profiles` 锁定这一点 | 前63B的两种动态状态均闭合：短 = C 字符串 + 写入端未初始化保留底层字节；长 = 标记 + Dept[0..58]。已知历史 join59 分叉不触及这63B，因此本段升完全闭环 |
| LBA6 | 0x03F | COMPLETE | Dept 内联最终字节 / join 判别项 / 短槽保留底层字节 | 当前 Linux/Windows/vrvaud 长写入端在此写 Dept[59]，当前76B Dept 原盘值为 GBK 尾字节 `A8`；短配置类型则只是固定64B槽的最后一个保留底层字节字节。历史 v19.11.4.1 `fcn.10006370` 是直接 LBA6 写入端，但其实现没有 `0x40245E2A` 长 Dept 标记，因此也不是 join59 写入端 | 当前长读取端用此字节区分接缝：非零则续段接 Dept[60]；为0则按旧版兼容分支从 Dept[59] 覆盖。CEMS2.0 x86 `fileophook.dll` 的 `fcn.10022f80@0x10022F80` 在 `0x100232ED` 比较标记，先复制 `0x3C=60B` 内联前缀，再把 LBA9 续段写到目标基址 `+0x3B`，从而覆盖 Dept[59]；x64 构建同构。这是 **CEMS2.0 join59 读取端仅**，不是写入端 | `scripts/protocol/audit_join59_semantic_closure.py` 用 Python 标准库直接校验固定 PE VA 机器码，并固定3×join59+4×join60 独立完整物理观测，全部重建为同一76B Dept（SHA-256=`4411ff1ca2294ad0953024244fd3423dada4a5a002c18be46cb5cb79fc86f970`）；独立 Lexar 测试夹具与已计入完整金标的 LBA6/LBA9 扇区逐字节相同，仅作回归、不重复计权。join59 本字节=NUL、join60 本字节=Dept[59]。另对12份短物理重放，Dept 均在第63字节前终止，而 `+0x3F` 同时出现 `00` 与非零 `8B`，证明短状态是 NUL 后保留底层字节 | 三种配置类型的逐字节角色现已唯一：短=NUL 后保留底层字节、join59=NUL 接缝、join60=Dept[59]。精确历史生成可执行文件/配置类型选择器仍未取得，但多个真实正向盘面 + x86/x64 双一方消费端已满足历史盘面级唯一逆映射门槛，因此实现来源不再构成本1B语义阻塞项 |
| LBA6 | 0x040–0x04F | COMPLETE | UsbMainBSec 静态模板材料 | Windows `sub_10013FD0` 先从 `UsbMainBSec@0x100E7220` 复制整扇；Linux `BuildSector6@0x1CAAC` 同样从 `UsbMainBSec@0x22BB40` 复制 sector_size；本16B没有后续覆盖项 | Windows `sub_100152A0` 与 Linux `ReadSector6@0x1E2CC` 都在字段解析前对原始 `+0x000..0x1FB` 计算并校验 SAFE6 校验和，不匹配即拒绝；字段解析器不另解释本段 | 严格22份原始盘解密后22/22精确等于官方模板 `f0 ac 3c 00 74 fc bb 07 00 b4 0e cd 10 eb f2 88`；CI含独立SanDisk锁定 | 固定写入端 + 整扇区 integrity 消费端 + 真实设备证据，无已知配置类型分叉，16B 完全闭环 |
| LBA6 | 0x050–0x06F | COMPLETE | `m_usbowner` / User 固定 32B 内联槽 | Windows `sub_10047690` 先以 `strcpy_s(dst=UsbWriteParam+0xFC, cap=0x9C, src=request.User)` 写156B User数组；`strcpy_s@0x10097F4F` 逐字节复制到首个NUL即停，不清剩余容量。注册时 `RegsiterUsb@0x1003B616` 再由 `sub_100139F0` 把持久 `UsbWriteParam` 复制到**未初始化栈局部 `var_3F4`**，该复制对 User 仍是同一个不清尾 `strcpy_s`。`BuildSector6` 对 `strlen<32` 固定复制该数组前32B；对 `strlen>=32` 写 `0x40245E2A + User[0..27]`，`User[28..NUL]` 写 LBA9+0x100 | Windows/Linux `ReadSector6`：短值只按 C 字符串消费到首NUL；长值检查标记后把前28B与 LBA9+0x100 续段重组。隔离 Unicorn 又直接执行当前 Windows `ReadSector6/sub_100152A0`：通过校验和/滚动后命中 `0x15552` 标记分支并把最大155B User 完整恢复到输出 `+0xFC` | 已提交原始样本的短 User 与 LBA8 `User=` 一致，且首个NUL后有真实非零保留底层字节；新增一方写入端测试夹具以最大合法155B User 运行 `BuildSector6@0x10013FD0`，解码后 inline28 精确等于 User[0..27]，LBA9 续段为127B剩余字符+NUL；动态官方读取端往返验证恢复原串。CI `official_virtual_long_user_uses_lba6_prefix_and_full_lba9_continuation_slot` 锁定 | 32B 的所有物理状态均闭合：短 = C 字符串 + 明确的写入端未初始化保留底层字节；长 = 标记 + 前28B。NUL 后非零不是隐藏字段，因此不再因“值不稳定”保留部分闭环 |
| LBA6 | 0x070–0x07F | COMPLETE | `m_autoid[16]` / Autonum 固定槽, 包括无语义 NUL 后保留底层字节 | Linux DWARF 定义 `UsbWriteParam.m_autoid char[16]@+0x259`；`UsbWriteParam(UsbLabelParam&)@0x1C362` 调自带 `strcpy_s@0x1B9B0`，该实现只复制到首个 NUL、**不清剩余容量**，且构造器入口没有先 memset 整对象；`BuildSector6` 随后固定 memcpy 完整16B 到 `LBA6+0x70` | `ReadSector6` 只用 `strcpy_s(...,16,decoded+0x70)` 消费首个 NUL 前的 C 字符串；`BuildSector8` 将该字符串序列化为 `Autonum=`；同时 LBA6 前508B 校验和覆盖并保护包括 NUL 后保留底层字节在内的全部物理字节 | 22/22 LBA6 C 字符串与 LBA8 Autonum 相同；已提交原始样本中同一个空字符串至少出现2种不同且非零的 NUL 后保留底层字节，直接证明尾字节不属于隐藏字符串语义 | 16B 的逐字节行为已闭合：前缀是 C 字符串，NUL 后是明确的 **写入端未初始化保留底层字节**；值不固定是协议实现行为本身，不是未知字段，因此整槽完全闭环 |
| LBA6 | 0x080–0x0BF | COMPLETE | `m_UsbOffice[64]` 固定槽, 包括无语义 NUL 后保留底层字节 | Linux DWARF 定义 `UsbWriteParam.m_UsbOffice char[64]@+0x198`；复制构造器用同一个不清尾 `strcpy_s@0x1B9B0` 写该数组且不预清对象；Windows/Linux `BuildSector6` 再固定复制完整64B到 `out+0x80` | Linux `ReadSector6@0x1E6A3..` 明确 `strcpy_s(UsbLabelParam.m_UsbOffice,64,decoded+0x80)`，只解释首个 NUL 前字符串；Windows 读取端同构；整扇校验和仍覆盖这64B的全部物理值 | 22份原始盘存在空/非空办公；已提交原始样本中同一个空办公字符串至少出现3种不同且非零的 NUL 后保留底层字节配置类型，排除隐藏字段/固定填充解释 | 64B 槽同样是 **写入端未初始化保留底层字节**：写入端缺陷、C 字符串消费端、完整性消费和多实盘配置类型均闭合；NUL 后值允许不稳定但无第二业务字段语义，故整槽完全闭环 |
| LBA6 | 0x0C0–0x0FF | COMPLETE | UsbMainBSec 静态引导代码/模板材料 | Windows/Linux BuildSector6 均先整扇复制官方 `UsbMainBSec`，本64B后续无字段覆盖 | 两端 ReadSector6 的 SAFE6 校验和在字段解析前覆盖整个前508B；本64B无独立业务字段读取 | 严格22份原始盘22/22逐字节等于官方模板；CI含独立SanDisk精确锁定 | 固定写入端 + 校验和消费端 + 22盘闭合，64B 完全闭环 |
| LBA6 | 0x100–0x103 | COMPLETE | 写入端负责 `m_crcUsbID[0]` 身份/密钥元数据 = CRC32(device_id) | Linux DWARF 明确 `m_crcUsbID[2]@CLabelManage+0x24`；Linux 构造函数/`Init` 都执行 `CRC32(0,m_strUID.c_str(),m_strUID.length()) -> +0x24`；Windows `sub_10013D20/sub_10013B80` 同构；两端 `BuildSector6` 都把该数组8B复制到扇区 +0x100 | 同源运行时成员 `m_crcUsbID[0]` 被 LBA7 滚动异或与 LBA8/LBA12 加解密直接使用；LBA6 当前 `ReadSector6`/Windows 当前 CheckLabel 正常分支不读取这个持久化副本，因此副本本身是校验和覆盖 / 语义忽略的写入端负责元数据，而不是当前输入参数 | 严格22份含独立SanDisk：22/22 `u32(+0x100)==CRC32(device_id)` 且全部非零；CI门禁 `lba6_crc_usb_id_pair_is_device_id_crc_and_doubled_guard` | 正式字段名、双平台写入端、值算法、同源运行时用途、LBA6 负语义消费端、整扇校验和所有权与真实盘均闭合；4B 完全闭环。未来读取端可继续忽略该副本而从设备 ID 重算 |
| LBA6 | 0x104–0x107 | COMPLETE | 写入端负责双倍 CRC 兼容保护值 = 2 × CRC32(device_id) mod 2^32 | Linux 构造函数/`Init` 直接 `m_crcUsbID[1]=m_crcUsbID[0]*2`；Windows两套构造路径同样 `object+0x48=object+0x44<<1`；`BuildSector6` 连续复制8B | Windows `CheckLabel/sub_100152A0`、`cemsudisk`、`vrvaud_c` 都保留 `+0x100!=0 && +0x104==(+0x100<<1)` 的历史一致性检查，并映射到版本/系统标签不匹配错误；当前构建的该值相关分支虽被恒真 `if(1)` 隔离，但正常读取端仍明确语义忽略该副本，整扇校验和覆盖其物理值 | 严格22份：22/22 `u32(+0x104)==u32(+0x100).wrapping_mul(2)`；独立SanDisk同样吻合；同一CI门禁锁定 | 写入端、历史消费端语义、当前负语义消费端、校验和所有权与真实盘关系均闭合；该4B是保留兼容保护值，不因当前分支不可达而继续视为未知业务字段 |
| LBA6 | 0x108–0x187 | COMPLETE | UsbMainBSec 静态引导代码/消息模板材料 | Windows/Linux BuildSector6 先复制官方 `UsbMainBSec`；本128B没有任何字段覆盖项 | Windows/Linux ReadSector6 均先校验覆盖前508B的 SAFE6 校验和；字段解析器不读取本段 | 严格22份22/22等于官方模板，包含 `Invalid partition table` / `Error loading operating system` / `Missing operating system` 旧版消息材料；CI含独立SanDisk锁定 | 128B 固定模板材料的写入端、完整性消费端、实盘闭合，完全闭环 |
| LBA6 | 0x188–0x1BF | COMPLETE | `m_usbLabel[64]` 前 56B 物理槽，包括无语义的 NUL 后保留底层字节 | Linux DWARF 明确固定 `UsbLabelParam/UsbWriteParam.m_usbLabel@+0x218`; `UsbWriteParam(UsbLabelParam&)@0x1C362` 使用内建 `strcpy_s(...,64,...)`，且该复制构造函数**不会**先 memset 0x299B 目标对象. 内建 `strcpy_s@0x1B9B0` 在复制首个 NUL 后立即返回，因此目标中 NUL 后字节保留此前底层内容. Windows `sub_10013FD0` / Linux `BuildSector6` 随后固定复制该数组前 0x38=56B 到 `out+0x188` | Linux `ReadSector6@0x1E84B..` 从对应位置构造 C++ 字符串 `decoded+0x188` 并写回 `UsbLabelParam.m_usbLabel[64]`; Windows 读取端同构. `BuildSector8` 把相同逻辑值序列化为 ELABEL `Label=`; LBA6 校验和覆盖该 56B 槽的每个物理字节 | 已提交原始样本都解码为相同业务值 `江苏电力!SAFE6`, 但其 NUL 后字节形成**至少 3 种不同且全部非零的保留底层字节配置类型**; 回归测试 `lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries` 锁定该事实. LBA6 C 字符串与 LBA8 `Label=` 在每份已提交原始样本中保持相等 | 完整 56B 行为已闭环：前缀是 C 字符串，NUL 后字节是从 64B 源数组复制来的**写入端未初始化保留底层字节**，不是隐藏字段或固定填充. 未来非零保留底层字节属于合法兼容数据；标准新盘制盘可以确定性清零并重新计算校验和 |
| LBA6 | 0x1C0–0x1C8 | COMPLETE | `m_usbGSerial` 的配置类型独立 C 字符串前缀 / 终止符位置 | 当前 Windows/Linux `BuildSector6` 都先清16B 临时，再从 `UsbWriteParam.m_usbGSerial` 固定复制前15B；已知短/长配置类型到 byte8 为止始终仍属于字符串本体：短为 `322CA28A\0`，长为 `322CA28A-` | Windows/Linux `ReadSector6` 从 `+0x1C0` 按 C 字符串解释；只有首个NUL之后才进入保留底层字节 | 严格 22盘：16份短=`322CA28A`、6份长=`322CA28A-D7D144`；完整历史去重统计集另复核20个前部，前8B 20/20=`322CA28A`，byte8 仅出现 `00` 或 `2D('-')`。旧版 MBR 底层内容存续字节从短 NUL 之后才开始 | 已知代际分叉不触及前9B的字段归属；9B从部分闭环升完全闭环 |
| LBA6 | 0x1C9–0x1CE | COMPLETE | GSerial 动态字符串尾部 / 调用方负责 NUL 后保留底层字节 | 当前 Windows/Linux 写入端对16B 临时清零后，固定从来源 `m_usbGSerial[0..14]` 复制15B；因此长配置类型时本6B可继续属于字符串正文，短配置类型时则只是来源 NUL 后保留底层字节。一方 Windows `BuildSector6` 虚拟执行把短 GSerial 的来源保留底层字节人为设为 `A5×6`，盘面逐字节保留，证明构造器不赋予第二字段语义 | Windows/Linux 读取端从 `+0x1C0` 仅按 C 字符串消费到首个NUL；短配置类型的本6B完全不参与业务解析，长配置类型则作为同一 GSerial 字符串尾部被正常消费 | 严格/当前/旧版实盘同时覆盖短/长；两份旧版中这些 NUL 后字节恰带旧 MBR 几何残值，只说明历史来源保留底层字节来源。新增官方虚拟测试夹具证明任意非零保留底层字节可合法往返验证到盘面 | 动态边界、写入端所有权、C 字符串消费端与非零保留底层字节正向证据均闭合；旧版 MBR残值不再被误当独立字段。6B从部分闭环升完全闭环 |
| LBA6 | 0x1CF | COMPLETE | `m_usbGSerial[15]` 专用全零终止字节 | Windows/Linux `BuildSector6` 都先把16B临时槽清零，只从来源 `m_usbGSerial` 固定复制前15B，因此 byte15 不受来源 NUL 后保留底层字节影响，始终保留显式零 | Windows/Linux `ReadSector6` 从 `+0x1C0` 按 C 字符串读取；当15B业务字符串占满前15B时该字节提供终止NUL，短串时仍只是固定槽尾零 | 已提交当前/旧版测试夹具全部为0；另对 `nopwd_tool/backup` 22份历史前部用 SAFE6 校验和过滤后 22/22 有效且 `+0x1CF=0` | 写入端所有权、C 字符串消费端、跨当前/旧版历史实盘均无分叉；1B从部分闭环升完全闭环 |
| LBA6 | 0x1D0 | COMPLETE | `BeiZhu` C 字符串首字节 / 空串 NUL | 当前 Windows/Linux 写入端从 BeiZhu 输入槽复制前15B到零化临时；读取端从 `+0x1D0` 按 C 字符串读回。无论配置类型是空串还是 GBK“普通”，首字节都仍属于字符串本体（空串时就是终止NUL） | Windows/Linux 读取端以 C 字符串解释，不存在首字节的仅底层内容分支 | 严格 22盘：20份首字节=00（空），2份首字节=C6（GBK“普通”首字节）；旧 MBR 底层内容在“普通”NUL之后才暴露 | 配置类型独立字符串边界闭合，1B升完全闭环 |
| LBA6 | 0x1D1–0x1DE | COMPLETE | BeiZhu 动态字符串尾部 / 调用方负责 NUL 后保留底层字节 | 当前 Windows/Linux 写入端同样只复制来源 BeiZhu 前15B到零化临时：非空配置类型中首个NUL前属于同一 BeiZhu 字符串，空/短配置类型中其余字节只是来源保留底层字节。一方 Windows `BuildSector6` 虚拟执行把空 BeiZhu 的来源 `+1..14` 人为填成 `5A×14`，输出逐字节保留 | Windows/Linux 读取端只按 C 字符串消费到首个NUL，绝不解释 NUL 后保留底层字节；旧版“普通”配置类型的 MBR几何残值位于NUL之后，因此同样不进入业务语义 | 22盘含20空+2个GBK“普通”，并有8/22 NUL 后非零；两份旧版保留底层字节与旧 MBR 快照连续。官方虚拟测试夹具又证明任意 `5A×14` 保留底层字节合法存在而不改变空字符串语义 | 与标签/办公固定槽同类：正文与保留底层字节由首NUL动态分界，保留底层字节是调用方负责兼容字节而非隐藏字段。14B从部分闭环升完全闭环 |
| LBA6 | 0x1DF | COMPLETE | `BeiZhu[15]` 专用全零终止字节 | 当前 Windows/Linux `BuildSector6` 对16B临时槽先清零、只复制来源 BeiZhu 前15B；历史 v19 也已补齐上游约束：`object+0x2620` 在全部可执行文件交叉引用中只有 `0x1000B219` 的写入端调用方读取和 `0x1000CCEF` 的唯一写入，后者明确执行 `strcpy_s(dest=object+0x2620, cap=0x10, source=object+0x2478)`。随后调用方先零化32B arg8，再以容量=32 从这个容量=16 对象字段复制，因此有效写入端最多15B正文+NUL，byte15 必为0 | 读取端从 `+0x1D0` 按 C 字符串消费；该字节是当前与 v19 两代合法写入端的固定安全终止NUL | 已提交当前/旧版测试夹具全部为0；扩展22份校验和有效历史前部同样22/22 `+0x1DF=0` | 上游容量=16 机器码直接消除了“v19 容量=32 可能让正文占据 byte15”的歧义；跨配置类型写入端/消费端/实盘完整闭合 |
| LBA6 | 0x1E0–0x1ED | COMPLETE | v19 BeiZhu 容量-32 C 字符串槽 NUL 后模板保留底层字节 / 旧版 MBR 条目3 快照片段 | 当前 Windows/Linux `BuildSector6` 的较新 ABI 只显式拥有到 `+0x1DF`；历史 v19.11.4.1 则把 `+0x1D0` 建模为 **容量=32 的 BeiZhu C 字符串槽**：`fcn.10006370@0x10006648..0x10006656` 对 `sector+0x1D0` 调 `strcpy_s(cap=0x20, caller arg8)`。其真实调用方 `0x1000B16C..0x1000B226` 先把完整32B局部清零，再从 `object+0x2620` 以 `strcpy_s(cap=0x20)` 填入 BeiZhu；进一步枚举全部可执行文件 `object+0x2620` 交叉引用只得到 `0x1000B219`（此处读取）与 `0x1000CCEF`（唯一写入），唯一写入在 `0x1000CCE8..0x1000CCF8` 明确执行 `strcpy_s(dest=object+0x2620, cap=0x10, source=object+0x2478)`。所以 v19 的合法源字段最多15B正文+NUL，32B arg8 的索引15 必为NUL、索引16..31保持先前零化；v19 `UsbMainBSec@0x101BA790` 的 `+0x1D0..+0x1F3` 也全部为0。因此该代写入端对本14B明确只能生成0，而不是主动生成 MBR 字段。旧版 Aigo+SanDisk+Netac 三个独立已提交配置类型则精确保留旧 MBR 条目3 字节[2..15]：起点 CHS尾、`type=0x07`、终点 CHS、start_lba、sector_count | v19 `ReadSector6@0x10006E07..0x10006E16` 从解码后 `+0x1D0` 调同一 `strcpy_s` 返回调用方 arg8，容量同为32，只消费到首个NUL。六个已恢复调用方分三组：第一组 arg8 局部除初始化/两次读取端传参外无后续引用；第二组成功路径解析的是另一 `esp+0x18` 字符串，不读取 arg8 的稳定 `esp+0x38` 输出槽；第三组 arg8 局部 `-0x64` 的唯一后续值使用是再经 `strcpy_s(cap=16)` 写入对象+0x140。因此 NUL 后 `+0x10..+0x1D` 没有 cmp/测试/哈希/分支/字段提取业务消费 | 20/22 严格历史为零；2/22 非零且类型/起点/数量与各自 LBA12 type4 精确对应；另一个独立来源已审计 `P-EESI-NETAC` 也为非零快照。`scripts/protocol/audit_legacy_lba6_mbr_underlay.py` 固定 Aigo、SanDisk、Netac 三种不同几何，三者启动/CHS/类型前8B一致，start_lba/sector_count 后8B随盘变化；`tests/provision_protocol_audit.rs` 进一步按各自设备 ID 解密 LBA12，并逐字锁定存活14B公式 `C1 FF 07 EF FF FF || LE32(type4 StartSector) || LE32(type4 PartionSize/512)`。真实免密 SanDisk 同一底层内容为16B全零；`scripts/protocol/audit_v19_lba6_beizhu_slot.py` 固定 v19 DLL SHA 并重放全零写入端/模板、读取端与调用方使用约束 | 消费端边界现已闭合，旧“v19 覆盖项不触及本14B”陈述已纠正；机器码证据继续**排除 v19.11.4.1 作为非零快照写入端**：已固定 v19 写入端/模板只能生成 NUL 后全零。`scripts/protocol/audit_netac_mbr_entry_boundary.py` 也排除当前 Netac 1.3.1.16 FormatExA 为历史非零来源。与此同时三种独立已提交非零几何都满足同一标准 MBR 条目3 / LBA12 type4 确定公式，全零配置类型又由当前/v19 一方写入端与物理盘独立覆盖，读取端对本段无独立业务消费。因此盘面字节含义、配置类型行为和标准新写规则已经唯一闭合，升级完全闭环；精确历史复制点/配置类型选择器继续作为实现/制造来源开放问题，不再冒充字节语义阻塞项 |
| LBA6 | 0x1EE–0x1EF | COMPLETE | 全零兼容尾部 / MBR 条目4 未使用前缀 | 当前 `UsbMainBSec` 对第4条条目的状态/起点头部为0且当前 BuildSector6不覆盖；v19 虽把 `+0x1D0..+0x1EF` 作为容量=32 BeiZhu目标槽，但其上游 `object+0x2620` 唯一写入是容量=16 C 字符串，调用方又先零化完整32B arg8，因此来源 NUL 后的 index30/31 必保持0；旧版 MBR 快照跨到 entry4 后实盘也为 `00 00` | ReadSector6只按 C 字符串消费至NUL，对这2B无独立值相关业务读取 | 已提交当前/旧版测试夹具均为0；扩展 `nopwd_tool/backup` 22份全部校验和有效，22/22 `+0x1EE..+0x1EF=00 00` | 当前/v19 写入端、旧版快照配置类型、负向消费端和全历史实证均无分叉；v19 容量=32 本身不再构成该2B边界歧义 |
| LBA6 | 0x1F0–0x1F3 | COMPLETE | 只写 `!SAFE` 标签代际元数据 (`m_encrypt`) | DWARF 正式定位 `UsbWriteParam.m_encrypt@+0x258`；Windows `RegsiterUsb` 对注册字符串执行5字节 `!SAFE` 匹配，相等/不等分支在 `0x1003BA94/0x1003BAC7` 分别写1/0，随后立即传给 BuildSector6；Windows/Linux BuildSector6 都把该布尔值扩成 DWORD 写 `+0x1F0` | 读取侧正式 `UsbLabelParam` 结构没有 `m_encrypt` 成员；Linux ReadSector6 不返回它。Windows `CheckLabel/sub_100152A0` 校验前508B 校验和后显式解析其它字段，但不读取 `+0x1F0`；已扫运行时无值相关消费端。因此消费语义是校验和覆盖 / 语义忽略，而非运行时加密开关 | 已提交严格原始样本 22/22=1；独立 SanDisk 原始 LBA6 同样=1；CI `lba6_m_encrypt_is_the_observed_write_only_safe_label_metadata` 锁定 | 写入端的0/1规则、正式字段名、负语义消费端、校验和所有权与真实盘均闭合。完全闭环不表示恒为1；非 `!SAFE` 写入端可合法写0 |
| LBA6 | 0x1F4–0x1FB | COMPLETE | UsbMainBSec 静态全零尾部 校验和前 | Windows/Linux BuildSector6 都由 `UsbMainBSec` 初始化，字段覆盖项最后只写到 `+0x1F3`，故8B保持模板零 | 两端 ReadSector6 的校验和覆盖到 `+0x1FB`；字段解析器无独立读取 | 严格22份22/22解密为8B零，官方模板同样为零；CI含独立SanDisk锁定 | 显式模板全零写入端 + 校验和消费端 +实盘，8B 完全闭环 |
| LBA6 | 0x1FC–0x1FF | COMPLETE | SAFE6 校验和 | 写入端对前508B计算校验和 | 读取端/检查校验 | 22/22 校验通过 | 完成 |
| LBA7 | 0x000–0x0BF | COMPLETE | 3×64B 紧凑布局旧版 EDPF 表 | Windows `CUsbRegsiter::CreatePartitions` 以紧凑布局 `0x40` 步长构造最多3条旧版条目；`edpediskctrl.dll::sub_100125B0` 将运行时 `0x60` 条目反向映射回紧凑布局旧版表，`SavePartionSector/sub_10028580 -> sub_10010FC0` 负责整表写回。标志、版本、PartionCount、PartionType、NeedDisturb、NeedEncrypt、StartSector、SectorSize、PartionSize、UserKeyCRC、FileKeyCRC 与旧版 wrapped8 的逐字段写入端生命周期均已在下方/详细审计独立闭合 | `ReadPartionInfoExEx/sub_10010B40`、旧版→新版转换器、`NewCheckDisTurbUsb(*)`、登录/挂载/改密与文件密钥 CRC 链按字段消费；版本与条目1/2 NeedDisturb 已由结构性保留 + 负向语义消费端闭合，wrapped8 已由解包+CRC 有效消费端闭合 | 22份严格原始样本全部按0x40 步长合法；另有独立真实免密 SanDisk 两条目配置类型。版本、NeedDisturb 按位置配置类型、wrapped8 正向解包/CRC 等均有已提交回归门禁 | 此行为**总括行**：192B 的逐字段证据已全部闭合，最终严格计数为192/192 完全闭环；Linux 自然对齐 `0x48` ABI只用于字段名/结构交叉，不得覆盖 Windows 物理偏移 |
| LBA7 | 每条条目 +0x004–+0x007 | COMPLETE | 条目内 `Version` 兼容元数据 | `CreatePartitions` 先清零3×0x40 紧凑布局旧版表，三条都没有 `+0x04` 覆盖写，因此当前写入端为0；Windows `0x40->0x60` 与 `0x60->0x40` 转换器均逐条结构保留该DWORD | 两版 `vrvaud_c` 对完整0xC0 紧凑布局表的行为交叉引用均不读取三条版本；协议代际由14B 密码信息版本决定。Linux 对应旧版→新版转换器也只结构搬运；`CDiskReader::GetTagPartitionInfo/DecryptFileKey/CheckFileKeyCrc/ReadFileSysSector0/DecryptFileSysSector0` 的真实消费字段分别集中在标志/PartionType/UserKeyCRC、StartSector、FileKeyCRC、封装密钥、EncryptMode，均不读取条目版本 | 已提交原始测试夹具 + 独立真实免密 SanDisk 的全部有效 LBA7 条目均 `Version=0`；扩展只读历史去重扫描仍无非零反例 | 不是保留，也不是 PartionCount；闭合的是“正式 ABI 兼容元数据，当前写入端=0、转换器结构性保留、运行时负向语义消费端”的完整生命周期，3×4B=12B 完全闭环 |
| LBA7 | 条目0 +0x010–+0x013 | COMPLETE | 条目0 `NeedDisturb` MBR 扰动/去扰门禁 | Windows `CreatePartitions` 对条目0 显式写入调用者传入的 `NeedDisturb=1`；旧版/新版 ABI 转换器双向保留该DWORD | 两版 Windows `vrvaud_c` 的 `NewCheckDisTurbUsb(*)` 直接检查紧凑布局条目0 `NeedDisturb@+0x10 != 0`；`SetProtect` 在该检查链后调用 `sub_1006ff80 -> sub_1006e580`，把 LBA0 `+0x1BE..+0x1FD` 的64B MBR表替换为静态扰动表；`UnsetProtect -> sub_1006ffd0 -> sub_1006e9b0` 则从 LBA2 读整扇恢复到 LBA0 并刷新磁盘属性 | 严格22份原始真实设备：22/22 条目0 `NeedDisturb=1`；另有真实免密 SanDisk 两条目配置类型同样条目0=1 | 字段不是泛化“防篡改”位，而是驱动侧是否进入系统可见 MBR 分区表扰动/去扰流程的门控；静态扰动表只有一条类型=0x04、start_lba=66、sector_count=1 的占位条目。条目1/条目2 的同名字段仍未找到独立消费端 |
| LBA7 | 条目1/条目2 +0x010–+0x013 | COMPLETE | 按位置 `NeedDisturb` 兼容元数据 | 当前 `CreatePartitions` 的调用参数固定为1：条目0/条目1 显式写1，条目2无覆盖写而继承整表清零0；Windows 旧版/新版转换器双向结构保留该DWORD | 两版 `vrvaud_c` 的行为读取只命中条目0 `NeedDisturb`；条目1/条目2 没有条件分支或参数映射。Linux 对应旧版→新版转换器保留字段，但 `CDiskReader` 文件系统检查链不读 NeedDisturb | 已提交原始测试夹具全量门禁按**位置**锁定三条目 `(1,1,0)` 与两条目 `(1,1)`；独立真实免密 SanDisk 的 type4 位于条目1 且值为1，证明该字段不是 `PartionType -> NeedDisturb` 恒等式；扩展历史扫描没有第三种按位置配置类型 | 8B 闭合为当前写入端按位置兼容配置类型 + 结构性保留 + 跨平台负语义消费端；完全闭环不把条目1/2 解释成条目0 的 MBR 扰动行为，也不禁止未来其它写入端配置类型 |
| LBA7 | 每条条目 +0x038–+0x03F | COMPLETE | 8B 旧版封装文件密钥 | Windows `sub_10028DB0` 以 `fold32(password)` 对两个32位半字做对称 XOR 包装；`sub_100125B0` 映射回旧版 0x40 条目；`SavePartionSector/sub_10028580 -> sub_10010FC0` 写 LBA7 | `sub_10026050` 对 v0x0064 固定解包8B，随后以 `sub_10038840` 计算 CRC32 并比较同条目 `FileKeyCRC(+0x34)`；改密后反向重包 | 22份原始真实设备中全部28条非零 type2/type4 旧版条目独立复算 28/28 PASS；默认 `fold32("0000aaaa")=0x91919191` | **LBA7 v0x0064 打包旧版文件密钥封装** 已闭合；FileKeyCRC 4B此前已经计入完全闭环，本轮仅新增3×8B=24B，禁止重复计数 |
| LBA7 | 0x0CA | COMPLETE | 密码信息 `bNoUsbChkPasSafe` / 官方 UI“取消密码复杂性验证” | `cemssafeudisklabeltool.exe`：`sub_4650a0` 以 `this+0x14` 为 `Ui_writeLabel` 基址；`Ui+0xE4=pwdComplexityCheckBox`，`retranslateUi/sub_487660` 的源字符串 `VA 0x4B5AD4` 精确为“取消密码复杂性验证”；`sub_466eb0` 调 `QAbstractButton::isChecked()` 原样写 `request+0x48`；`sub_42e8e0` 再原样写 `LabelInfo+0x7EC`。既有 Windows `WriteNormalULabel -> CreatePartitions` 链把 `UsbWriteParam+0x7EC` 写入 `PassInfo+0x0A` | 独立 Linux 官方 `checkdiskback::Update_EDPEDISKSHOWPARAM@0x406B70` 直接执行 `cmp byte [pass+0x0A],1; setne showparam+0x03`，随后 `CreateSafe6TmpPolicyFile@0x407D50` 将结果纳入 CRC/加密 SAFE6 策略；独立 `EdpEDiskBack::Safe6PolicyFile::GetSafe6Policy@0x4100B0` 与 `linuxedpedisk::Safe6PolicyFile::GetSafe6Policy@0x41EAA0` 解密并恢复该策略/运行时参数 | 严格22份原始盘：18×0、4×1；22/22 LBA7/LBA12 同盘取值一致；CI `pass_info_no_usb_safe_flag_varies_and_matches_between_lba7_and_lba12` 锁定0/1双值与跨扇区一致性 | **语义方向已由官方 UI 闭合**：未勾选=0；勾选“取消密码复杂性验证”=1；值在 UI→请求→LabelInfo→PassInfo 链不取反。保留 ABI 原名 `bNoUsbChkPasSafe`，人类可读语义解释为“取消/跳过密码复杂性验证” |
| LBA7 | 0x0CC–0x0CD | COMPLETE | 未启用密码信息 `ShareBackuppromptPeriod / EncryptBackuppromptPeriod` 兼容字节 | Linux DWARF `edpdiskglobal.h:164/165` 明确给出两个独立 `BYTE` 字段，物理偏移 `+0x0C/+0x0D`；当前 `CreatePartitions/sub_1003DB50` 在 `0x1003DC16..0x1003DC26` 显式清零完整14B 密码信息，后续存储只到 `+0x0A`，因此当前写入端为0/0 | 四个不同哈希/代际的 `EdpEDiskCtrl` 读取端均把完整14B 密码信息结构复制到输出；已复核成功尾部只对 `Version(+0)`、共享重试 `(+3)`、加密重试 `(+6)` 做 XOR/值处理，`+0x0C/+0x0D` 只结构性保留。Linux 检查器同样保存完整14B但无这2B业务读取；两代 `vrvaud_c::BackupPromptInfo/BackupStartTime/BackupEndTime` 已证明是独立策略/字符串/DWORD 链，与密码信息无数据流 | 已提交原始样本的 LBA7/LBA12 两份副本逐盘0/0且一致；全目录去重扫描19个真实 LBA7 密文配置类型（覆盖密码信息 v0x0064 与 v0x0206）仍19/19=0/0 | 闭合语义是“正式命名但在已覆盖实现中未启用的兼容字节”：写入端=0、读取端结构性保留/负向语义消费端、跨代/跨LBA实盘一致。完全闭环不声称其历史设计单位是小时/天；未来非零配置类型必须保留并扩展，不得机械清零 |
| LBA7 | 0x0CE–0x1FF | COMPLETE | 紧凑布局旧版表表后写入端负责的全零区域 | Windows `edpediskctrl.dll::sub_10010FC0` 先以 `sub_1004D110(...,0,0xFFF)` 明确 memset 暂存，随后只复制 `0xC0` 紧凑布局表 + `0x0E` 密码信息，再对完整512B 滚动并写 LBA7；`sub_1004D110` 机器码已复核为 memset 等价实现 | Windows `ReadPartionInfoExEx/sub_10010B40` 解密完整512B，但成功后只复制 `0xC0` 表和 `0x0E` 密码信息，完全不返回/解释 `0x0CE..0x1FF`；Linux 自然对齐 ABI 构造器也独立采用“整块清零→写结构→整扇滚动”的同原则，但其表尾在0xE6，只作原则佐证、不用于覆盖Windows物理偏移 | 严格22份原始生成参考（21 未转换备份 + 独立SanDisk）逐盘解密：22/22 `0x0CE..0x1FF == zero[306]`；CI门禁 `lba7_post_table_plaintext_is_zero_through_sector_end` 锁定已提交原始 subset | 306B 的写入端零来源、负向消费端、物理边界和原盘均闭合；这里的完全闭环表示写入端负责全零区域，不是靠“样本碰巧全零”推断 |
| LBA8 | 0x000–0x003 | COMPLETE | LLGB 魔数 | Windows/Linux `BuildSector8` | 读取端先检查 LLGB | 22/22 | 完成 |
| LBA8 | 0x004–0x007 | COMPLETE | 逻辑长度 | 写入端=`0x80+strlen(ELABEL)` | 解码器决定动态加密前缀 | 22/22吻合 | 完成 |
| LBA8 | 0x008–0x00B | COMPLETE | ToolVersion[4] | Windows `sub_100148d0` 与 Linux `BuildSector8@diskfile.cpp:805` 都写固定字节 `01 00 00 01` | `ReadSector8(UsbLabelParam&)` 的语义解析器不读取该版本戳；`ReadSector8(BYTE*)` 仅把完整解密扇区原样导出 | 22/22原始盘=`01 00 00 01`；CI原始夹具锁定 | 4B 写入端、读取端行为、实盘一致，无已知配置类型分叉 |
| LBA8 | 0x00C–0x00F | COMPLETE | Labversion | Windows/Linux 写入端都固定写 `0x00000222` | 语义解析器跳过该 DWORD；原始读取端仅原样导出 | 22/22原始盘=`0x222`；CI原始夹具锁定 | 4B 标签版本戳闭合 |
| LBA8 | 0x010–0x013 | COMPLETE | writeTime | Windows 写入端调 `GetTickCount()`；Linux `CLabelManage::GetTickCount@0x1FBAA` 用 `clock_gettime(CLOCK_MONOTONIC)` 转为毫秒并截为32位 | 两个官方读取端都不把该值用于标签解析/准入；原始读取端只导出原值 | 22/22原始盘均非零且跨标签变化；CI夹具保持多值反例 | 不是墙钟时间，而是制标时单调时钟毫秒计数（32位回绕） |
| LBA8 | 0x014–0x017 | COMPLETE | HDSerialInfo / 镜像主机身份信息 身份 DWORD | `tagEdpUsbLableInfo.HDSerialInfo@+0x14`。当前 Windows/Linux BuildSector8 从零初始化头得到0；另取得并哈希核验 2020 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5 `783d01f19e998a514834bc5e5f4249ad`），其可达 ELABEL 写入端 `sub_10007DF0` 先调 `UsbTools.dll` ordinal4，结果为0才回退 ordinal3，并把返回DWORD写到临时LLGB结构 `+0x14`。本机 `UsbTools.dll` 导出表已精确证明 ordinal4=`EDP_DiskNumber`、ordinal3=`EDP_DeviceNumber`；两者分别是跳到 `DeviceNumber.dll` ordinal3/ordinal1 的纯 thunk。`EDP_DiskNumber` 对 PhysicalDrive ATA 序列号规范化后做反射形式 CRC32；`EDP_DeviceNumber` 复用同一序列号流，再按接受顺序追加 `MACAddress<i>=<12位大写无分隔MAC>\r\n`，至少一条时追加 `MACCount=<N>\r\n`（排除非零条件不满足及描述同时命中虚拟+VMWARE 的虚拟适配器），随后对 `serial_blob` 与 `mac_blob` 拼接做同一 CRC32 | 注册侧语义读取端不读取该DWORD；当前运行时 `sub_10016260` 与2020 `EdpEDiskCtrl.dll` 都结构保存它但未发现值相关行为读点。新增跨扇区证据：LBA4 恢复节点的 `MyHardinfo` 在22份严格原始样本中逐盘与本DWORD完全相等 | 当前配置类型全0；旧版配置类型全非零并按捕获/硬件环境成组；**22/22 `LBA4.MyHardinfo == LBA8.HDSerialInfo`**，且非零集合精确包含 `A017AD78/A68BAE08/8B4613F5/2AB0E33C`。2020 写入端证明存在“宿主磁盘/主机身份 CRC32 -> HDSerialInfo” 写入端家族 | 字段级生命周期闭合为跨LBA镜像的主机身份信息 身份 DWORD：v19 官方写入端对 LBA4/LBA8 分别独立调用同一 `EDP_DiskNumber`/回退 `EDP_DeviceNumber`，22/22 严格物理镜像一致，非零值按宿主环境成组且 `A68BAE08` 跨 Lexar/Aigo 不同目标U盘出现，运行时消费端无值相关分支。相邻 UsbOnlyInfo 的代际分叉继续单独记账，不再阻塞本4B，故升完全闭环 |
| LBA8 | 0x018–0x01D | COMPLETE | `MacInfo[6]` 保留/未使用 MAC 槽 | Linux DWARF 正式定义 `MacInfo unsigned char[6]@+0x18`；Windows/Linux BuildSector8 都先清零完整头部，Linux 再把仍为0的 DWORD+字写到+0x18，当前写入端明确为6B零 | Windows `sub_10015820` 与 Linux `ReadSector8(UsbLabelParam&)` 均从 ElabOffset 解析 ELABEL，不读取 MacInfo；原始读取端仅不透明导出，不赋予业务语义 | 严格22份原始盘跨当前/旧版身份 **22/22均为6B零**；CI `lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty` 现对全部配置类型锁定 MacInfo=0 | 官方字段边界、显式零写入端、负语义消费端与跨代实盘均闭合，无已知配置类型分叉，6B 完全闭环 |
| LBA8 | 0x01E–0x02D | COMPLETE | `UsbOnlyInfo[0..15]` 可选 16 字符兼容身份文本 | 当前 Windows `RegsiterUsb -> sub_100148d0` 明确以主 onlyid 执行 `sprintf("%08x%08x", onlyid,0)`；Linux BuildSector8同构。2020 `sub_10007DF0` 使用相同16字符模板，但第二DWORD来自 `EDP_DiskNumber`/回退 `EDP_DeviceNumber`，因此有效写入端配置类型至少包含当前 `main-onlyid + 0` 与过渡 `main-onlyid + host-hardinfo`；严格旧版物理配置类型则为该槽缺失/全零 | 注册语义读取端完全跳过该槽；当前运行时 `sub_10016260` 与2020 `EdpEDiskCtrl.dll` 只结构保存，未发现值相关行为读点；Linux `ReadSector8(UsbLabelParam&)` 从 ElabOffset 解析 ELABEL，不用 UsbOnlyInfo 做准入或业务决策 | 已提交原始样本中当前身份精确等于 `format("%08x%08x", main_onlyid_bits,0)`，严格旧版为16B全零；2020 官方写入端又证明第二个8字符组可合法承载非零主机身份信息 | 字段级生命周期按可选兼容槽闭合：两种有效写入端、严格旧版缺失全零物理配置类型、跨代负语义消费端均已明确。完全闭环不声称严格旧版曾执行 v19 格式化，也不把缺失配置类型反推成某个未取得的精确可执行文件；未来未知非零格式必须按新配置类型保留 |
| LBA8 | 0x02E–0x03D | COMPLETE | `UsbOnlyInfo` 固定 C 字符串终止符 + 全零后缀 | 当前 Windows/Linux 写入端均生成**恰好16字符** `%08x%08x`，目标32B 头槽来自完整零初始化，故第16字节是终止NUL、其后15B保持零；2020官方 `sub_10007DF0` 同样先清零完整临时标签结构，再对32B槽执行同一16字符格式化，因此后缀同为 `00[16]`。严格旧版配置类型整槽缺失/全零，自然保持相同后16B | 注册语义读取端完全跳过 UsbOnlyInfo；2020/当前运行时最多结构复制该槽，没有后缀值相关消费端；另一本机 v3.6.12.28 运行时甚至只复制槽首DWORD，直接跳过这16B 后缀 | 已提交严格原始样本 22/22 解密后 `+0x2E..+0x3D==zero[16]`，跨当前/旧版身份无反例；现有回归再显式锁定通用后缀全零 | 16B不存在已知配置类型分叉，且当前/2020 写入端零来源、旧版缺失配置类型、跨代负向/结构性消费端与实盘均闭合；未来若出现非零后缀必须新增配置类型，不得机械清零 |
| LBA8 | 0x03E–0x03F | COMPLETE | ElabOffset | `BuildSector8@diskfile.cpp:805` 写 `0x0080`；官方结构 `tagEdpUsbLableInfo.ElabOffset@edpdiskglobal.h:413` | `ReadSector8@diskfile.cpp:1102` 读取字并用 `decoded+ElabOffset` 构造 ELABEL 字符串 | 22/22原始盘=0x80，且22/22都指向 `<ELABEL>`；CI真实夹具锁定 | 2B 寻址语义、写入端、消费端、实盘全部闭合 |
| LBA8 | 0x040–0x07F | COMPLETE | Reserverd[64] | Windows `sub_100148d0` 与 Linux `BuildSector8` 都先零初始化整个头，再把未被其它赋值覆盖的 64B 原样复制到该区 | `ReadSector8(UsbLabelParam&)` 直接越过该区定位 `ElabOffset` 指向的 ELABEL；原始读取端仅原样导出，不赋予业务语义 | 22/22原始盘解密后64B全零；CI原始夹具锁定 | 官方结构名、零初始化写入端、负向消费端和实盘全部闭合为保留全零区 |
| LBA8 | 0x080–0x1FF | COMPLETE | **LBA8 动态 ELABEL + 加密后的保留底层字节 + 保留尾部** | Windows `sub_100148d0` 与 Linux `BuildSector8@0x1D602` 独立同构：都只把 17 个键 ELABEL+NUL 写到 `+0x80`，按 `(logical_len / 16 + 1) * 16` 对现有 LBA8 **原地**加密；两函数都不清调用方输出。Windows `RegsiterUsb` 在此之前已先读完整13扇区到 `var_500`，再把旧 `LBA8` 指针直接传入写入端，因此 ELABEL NUL 后到 `encrypted_len` 的字节属于既有保留底层字节、会随前缀一起加密，`encrypted_len..0x1FF` 则完全原样保留现有 | Windows 注册侧 `cemsusbregsiter.dll::sub_10015820` 与 Linux `ReadSector8(UsbLabelParam&)` 独立只回填同一 7 键集合：`registration semantic reader key set = Label/GLab/Dept/User/Autonum/Rmark/Unit`。但运行时 `out_raw_data/EdpEDiskCtrl.dll::sub_10016260` 是更完整的消费端：`runtime EdpEDiskCtrl reader parses all 17 ELABEL keys`，将 `GLab/Indus/Orgcd/Org/Unit/Dept/User/Alarm/Autonum/Label/Rmark/VOL0/1/2/VOLC0/1/2` 全部写入 `tagEdpUsbLableInfo` 对应槽。NUL 后块内保留底层字节和块外尾部不赋予字段语义；检查的非零 块内保留底层字节、非零物理尾部、16B 对齐额外块三个回归锁定兼容读取边界 | 严格22份原始盘：`logical_end=0x148..0x183`、加密前缀=`0x150..0x190`，22/22 正文均为同一17 个键顺序且十个当前兼容键为空；已提交原始门禁同时覆盖多个动态长度，并验证当前实盘 `ELABEL NUL 后到 encrypted_len 的字节属于既有 backing` 的观测值目前为零。所有观测物理尾部也为零，但两者的零值都不是协议要求 | 384B 的动态状态机已逐类闭合：正文/终止NUL由写入端拥有；块内剩余字节为加密保留保留底层字节；块外为未加密保留尾部。7个核心键有注册侧 Windows/Linux 双消费端，运行时 EdpEDiskCtrl 又对全部17键提供完整结构消费端；实盘无已知正文/配置类型分叉。因此整段384B升完全闭环；这不要求保留底层字节/尾部恒零，未来非零值必须按边界原样保留 |
| LBA9 | 0x000–0x003 | COMPLETE | EETU 魔数 | `CUsbRegsiter::SetTempUse` 构造 `EETU`；`WriteTempUseInfo` 可运行时回写 | `ReadTempUseInfo` 必须校验 EETU 魔数 | 20个非零LBA9原始样本 | 完成 |
| LBA9 | 0x004–0x00B | COMPLETE | ullBTime | Windows `SetTempUse` 从开始时间字符串解析为64位值；空/短字符串保持0 | Linux `CheckTempUse` 与 `time(NULL)` 比较；非零且当前时间 < ullBTime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 开始时间下界语义闭合，0表示不启用该下界 |
| LBA9 | 0x00C–0x013 | COMPLETE | ullETime | Windows `SetTempUse` 从结束时间字符串解析为64位值；空/短字符串保持0 | `CheckTempUse` 与 `time(NULL)` 比较；非零且当前时间 > ullETime 时拒绝临时使用 | 20/20原始EETU=0；真实CI夹具回归 | 结束时间上界语义闭合，0表示不启用该上界 |
| LBA9 | 0x014–0x017 | COMPLETE | useCount | `BusManageImp::WriteNormalULabel` 普通模式从请求 `+0x947` 取次数；特殊 OutManage 关闭模式明确写 `0xFFFFFFFF`；`CUsbRegsiter::SetTempUse` 再将请求+0x40 原样写 EETU+0x14 | Linux `CheckTempUse`：`0xFFFFFFFF` 不递减/不回写；0=次数耗尽；其它正值减1并 `WriteTempUseInfo` 回写 | 20/20原始EETU=0xFFFFFFFF；真实CI夹具回归 | 4B 次数控制及无限次数哨兵完全闭合 |
| LBA9 | 0x018–0x07D | COMPLETE | **EETU reverse[0..101] 写入端未初始化不透明保留底层字节** | Linux DWARF `tagEdpEDiskTmpUse@edpdiskglobal.h:481` 正式定义 `reverse[104]@+0x18`。Windows `CUsbRegsiter::SetTempUse` 先清零 EETU 魔数后124B，再固定 `memcpy(EETU+0x18, request+0x44, 0x66)`。继续回溯 `BusManageImp::WriteNormalULabel/sub_100A99F0` 原始机器码确认：SetTempUse 请求 `&var_BD4` 只写 `begin[32]`、`end[32]`、`useCount@+0x40`；此前 `sub_1009BAD0` 的 `ECX=&var_9EC` 只清 `ebp-0x9EC..-0x56`，并不覆盖位于 `ebp-0xBD4` 的请求对象，因此 `request+0x44..+0xA9` 102B 是明确的写入端未初始化调用方保留底层字节，而非保留全零 | 当前 Windows 运行时 `ReadTempUseInfo/sub_10013490` 解密并把完整0x80 EETU缓存到 `CEdpDiskControl+0x1076`；行为代码 `GetTempUseInfo/CheckTmpUse` 只直接读取 `ullBTime/ullETime/useCount`（显式对象引用截止 `+0x108A`，reverse 从 `+0x108E` 开始无独立读点）。登录递减次数后 `WriteTempUseInfo/sub_10013770` 将完整0x80重新加密写回，所以 **运行时保留完整 0x80 EETU，同时只消费 time/useCount**。Linux `CheckTempUse` 同样不读取 reverse | 20/20原始非零LBA9的 EETU `reverse[104]` 全零；已提交原始门禁 `real_eetu_temp_use_limits_match_the_official_unlimited_profile` 锁定该观察，扩展历史只读扫描也未发现非零 reverse | 前102B的“值不稳定/当前恰零”本身不是未知字段：正式边界、写入端未初始化来源、运行时透明保存/负语义消费端与原始实盘均闭合，按与 LBA6/LBA8 保留底层字节相同口径升完全闭环。未来若出现非零 reverse 必须原样保留，不得清零或赋予隐藏字段语义 |
| LBA9 | 0x07E–0x07F | COMPLETE | reverse[102..103] 全零尾部 | `SetTempUse` 对 EETU +0x04..+0x7F 先整体清零，随后从 +0x18 只覆盖0x66B，即最后覆盖到 +0x7D；因此 +0x7E/+0x7F 在所有当前写入端路径都保留显式零初始化 | Linux `CheckTempUse` 只读取 ullBTime/ullETime/useCount，对 reverse[104] 完全无业务读取；运行时回写只修改 useCount 并保留其余字节 | 20/20原始EETU均为 `00 00`；CI门禁 `lba9_eetu_final_two_reverse_bytes_are_writer_zero_padding` | 2B 满足显式全零写入端 + 负向消费端 + 真实设备证据，可严格升完全闭环；不得把前102B一起升级 |
| LBA9 | 0x100–0x103 | COMPLETE | SAPF 魔数 | 旧写入端恢复模板 | `UDiskLabelRepair::Repair0Sector` | 14样本 | 完成 |
| LBA9 | 0x104–0x113 | COMPLETE | MBR恢复条目 | 写入端保存16B 条目 | 修复直接写回 LBA0 0x1BE | 14/14 | 完成 |
| LBA9 | 0x114–0x11F | COMPLETE | **配置类型重叠区域：SAPF 无所有者尾部保留底层字节 / 长 User 续段** | SAPF 配置类型下历史写入端虽未定位，但该12B没有独立字段存储证据；真实解码后尾部呈现零、`0xFFFFFFFE` 与多组 `0x77xxxxxx` 等典型保留底层字节形态。当前长 User 配置类型则由 Windows/Linux/vrvaud 三套 `BuildSector6` 明确把 `User[28..NUL]` 连续写入 LBA9+0x100，最大155B User 的一方虚拟写入端正例覆盖整个 `+0x100..+0x17F`，因此本12B在该配置类型下是确定的 User 续段负载 | SAPF `sub_10008550` 虽结构性复制解码完整32B，但只校验魔数；上层双SAPF一致性只比较解码后 `+0x04/+0x08/+0x0C/+0x10`，`Repair0Sector/sub_10008620` 又只把这四个DWORD写回 LBA0 `+0x1BE..+0x1CD`，对 `+0x14..+0x1F` 12B零读取/零写回。长 User 配置类型则由 `ReadSector6` 固定从 LBA9+0x100 取0x80B 续段回填 User | 14份真实SAPF中解码后尾部 12B 至少5种配置类型，CI同时要求全零/非零两类都存在，排除“协议固定零”；最大155B 官方虚拟长 User 测试夹具又给出同一物理12B的有效负载正例并完整往返验证 | 两种已知配置类型均已闭合：SAPF下是**无所有者尾部保留底层字节 + 负语义消费端**，长 User下是有效续段。历史 SAPF 最初保留底层字节来源不再是业务语义阻塞项；兼容实现必须按配置类型解释，禁止把SAPF 尾部强制清零或把长 User字节当SAPF字段 |
| LBA9 | 0x120–0x17F | COMPLETE | 长 User 续段剩余部分 / 其他情况下保留底层字节 | 三套当前写入端的机器码已逐一核对为同构：Windows `0x1001417D..0x100141B1`、Linux `0x1CDA3..0x1CDE2`、vrvaud `0x10119082..0x101190B5` 都计算 `out + 3*sector_size + 0x100`，写入 `User[28..NUL]`，长度=`strlen(User)-27`；User固定数组容量0x9C允许最大155B C 字符串，因此最大写长恰为128B并止于+0x17F。短User路径不触碰该区，保留既有保留底层字节 | Windows `ReadSector6/sub_100152A0` 标记分支固定读取 LBA9+0x100 的0x80B到 User 续段；Linux 读取端同构。SAPF 消费端明确止于+0x11F，EETU/EPPE运行时也不消费+0x120..+0x17F | 一方 Windows `BuildSector6` 隔离执行：LBA9预填0xCC，最大155B User 后 `+0x100..+0x17F` 被精确写满“127B剩余字符+NUL”，`+0x000..0x0FF` 与 `+0x180..` 仍保持0xCC，证明真实写边界；同一盘面再由官方 `ReadSector6` 动态执行完整校验和/滚动/标记路径，输出155B原串完全一致。CI 测试夹具锁定。物理原始样本虽无长User，但14/14 SAPF盘与独立SanDisk在该区为零的原样保留配置类型仍保留 | 96B 已有一方写入端→盘面→消费端正向闭环，并证明其他情况下原样保留生命周期；虚拟测试夹具与物理统计集继续分栏，因此本段升完全闭环 |
| LBA9 | 0x180–0x183 | COMPLETE | EPPE 魔数 | `SetPassInfoEx` | `ReadPassExInfo` | 6样本 | 完成 |
| LBA9 | 0x184–0x187 | COMPLETE | minimum 密码长度 | 写入端限制6..19 | `ReadMinPassLenInfo` 返回该DWORD | 6/6=8 | 完成 |
| LBA9 | 0x188–0x1FF | COMPLETE | **EPPE 写入端负责全零尾部** | PE机器码 `SetPassInfoEx/sub_1003ADD0`：先校验输入DWORD为6..19，再对 EPPE `+0x04..+0x7F` 124B整体清零，写魔数，随后明确 `EPPE+0x04=*arg0`；因此 `+0x08..+0x7F` 120B 在当前写入端中为显式零 | 正式注册侧 `CUsbRegsiter::GetPassInfoEx` 解密并校验 EPPE 后只执行 `*out = *(decoded+0x04)`；独立 `modfilesyscheck::ReadMinPassLenInfo` 同样只消费魔数/+0x04。两套 `EdpDiskCtrl` 虽保留可搬运完整0x80B的 `ReadPassExInfo/GetPassExInfo` 兼容辅助函数，但其外层对象由唯一两个 DLL 导出（创建/发布）创建，当前工厂虚表 `0x1008021c` 不包含该辅助函数，反编译交叉引用也仅见实现/相邻 thunk，未形成当前产品语义消费路径 | 严格22份中6份EPPE；6/6 minPassLen=8 且解密后120B全零；CI门禁 `real_eppe_samples_keep_the_current_writer_zero_tail` | 120B 的当前写入端、两个独立语义读取端的负向消费端、当前公开接口边界与原始实盘均闭合，按写入端负责全零区域升完全闭环。完全闭环不授权清洗未知历史非零配置类型：兼容读取若未来遇到非零尾部应保留/报告，而不是据此臆造业务字段 |
| LBA9 | 0x080–0x0FF | COMPLETE | 长 Dept 续段 C 字符串 + NUL 后无所有者保留底层字节（join59 / join60） | Windows `BuildSector6/sub_10013FD0`、Linux `BuildSector6@0x1CAAC` 与 `vrvaud_c::sub_10118ED0` 三套当前写入端一致：Dept长度>=64时在 LBA6+0写 `0x40245E2A + Dept前60B`，再把 `Dept[60..NUL]` 写入 LBA9+0x80；若未触发长Dept则该构造器不覆盖此区。对本机 VRV 二进制按 `0x40245E2A` 做全量常量指纹后，写入端命中仅见当前 `cemsusbregsiter.dll` / `vrvaud_c.dll`，且两者都复制60B；历史 v19.11.4.1 直接 LBA6 写入端 `fcn.10006370` 则完全没有标记分支 | 当前 Windows `ReadSector6/sub_100152A0` 先从标记后复制完整 `0x3C=60B` 内联前缀，再检查 `prefix[59]`：0时续段接 Dept+59，非零时接 Dept+60。CEMS2.0 `cems/Edp/fileophook.dll` `fcn.10022f80@0x10022F80` 的标记分支在复制60B 内联后，无条件把0x80 续段写到目标 `+0x3B`；x64 构建同构，PDB路径明确落在 `\\SVNRoot\\vrvrsms2.0\\Cems2.0\\trunk\\modCems\\Bin\\FileOpHook*.pdb`。因此这里闭合的是 **CEMS2.0 join59 读取端仅**，不能反推同代写入端 | `audit_join59_semantic_closure.py` 固定3×join59+4×join60 独立完整观测：两组均重建为相同76B Dept；独立 Lexar 测试夹具与已计入完整金标的对应 LBA6/LBA9 扇区相同，只作回归夹具。join59 续段 NUL 索引=17、其后110B当前全零，join60 NUL 索引=16、其后111B当前全零。CI `lba9_dept_continuation_preserves_both_official_reader_join_profiles` 继续锁定内联[59]!=0 -> 60、内联[59]==0 -> 59；NUL后正式建模为消费端忽略 保留底层字节，而非协议强制零 | 128B 的动态续段 C 字符串、join59/join60 接缝和 NUL 后保留底层字节生命周期均已闭合。精确历史 join59 生成可执行文件/配置类型选择器继续作为实现来源；多个真实正向盘面 + x86/x64 双独立一方消费端已唯一确定旧盘面映射，因此该来源不再是字节语义阻塞项 |
| LBA10 | 0x000–0x003 | COMPLETE | EESI 魔数 | `SetEdpEdiskSetInfo` 强制写 `EESI` 魔数，并只加密/覆盖前0x80B | `GetEdpEdiskSetInfo` 解密前0x80B并首先校验 `EESI` | 特定用途 Netac 写入前物理采集 SHA-256 `3c7e795b...` 在 `CRC32(device_id)=0x5088EE37` 下解出 `EESI`；19+1 通用统计集则保持缺失全零配置类型 | 写入端、读取端、正向/缺省两类真实物理配置类型均闭合；特定用途采集不参与通用统计集计数 |
| LBA10 | 0x004–0x007 | COMPLETE | **UsbSuspensionWnd 生命周期/控制标志** | 两套独立 `EdpEDisk.exe`（SHA-256 `cfa13177...` / `dc71c300...`）启动时都先将完整0x80B EESI缓冲清零并显式写 `+0x04=1` 后调用虚表 `+0x20 GetEdpEdiskSetInfo`；同两套程序的卷标设置对话框 `IDOK` 处理函数将完整 0x80B EESI 负载清零，只填 `+0x08/+0x18` 两个卷标，再经虚表 `+0x24 SetEdpEdiskSetInfo` 保存，因此该写入端路径明确写 `+0x04=0`；底层设置函数只强制魔数，其余DWORD原样落盘 | 两套程序均加载 `UsbSuspensionWnd.dll` 的 `Show/Destroy/SetParentWnd`：读回 EESI 后 `+0x04==0` 路径调用 `Destroy`；自动登录成功后 `+0x04!=0` 且挂起窗口辅助函数已初始化时，进入刷新/`Show` 链；`UserLogin` 自身不把该DWORD当卷标开关 | 来源已审计 Netac 写入前采集解出 `+0x04=1`；通用统计集提供缺失全零配置类型 | 4B边界、0/1官方写入端、值相关消费端与正向物理值全部闭合 |
| LBA10 | 0x008–0x017 | COMPLETE | 共享/type2 卷标 | `SetEdpEdiskSetInfo` 原样复制调用者结构前0x80并加密写入；默认读取端初始化为GBK“交换区” | `CEdpDiskControl::UserLogin` 将该槽赋给本地字符串；type2分支传给 `SetVolumeLabelA` | 来源已审计 Netac 写入前采集解出固定16B槽 `bdbbbbbbc7f8...` = GBK“交换区” | 16B边界、写入端、业务消费端与正向真实盘值闭合 |
| LBA10 | 0x018–0x027 | COMPLETE | 加密/type4 卷标 | 同上；默认读取端初始化为GBK“保密区” | `UserLogin` type4分支传给 `SetVolumeLabelA` | 来源已审计 Netac 写入前采集解出固定16B槽 `b1a3c3dcc7f8...` = GBK“保密区” | 16B边界、写入端、业务消费端与正向真实盘值闭合 |
| LBA10 | 0x028–0x07F | COMPLETE | EESI 调用方负责的兼容扩展 | 两个独立 EESI `EdpEDiskCtrl` 构建的读取/集合证明完整0x80B 结构性往返验证；当前官方 UI 配置类型对这88B写零 | 当前 callers不读取这88B；更老两代无EESI接口 | 来源已审计 Netac EESI 启用采集与独立旧 SanDisk 正例均为88B全零 | 生命周期边界已闭合：调用方可往返验证扩展字节，当前官方调用方写零且业务消费端不解释；完全闭环不表示未来扩展必须恒零 |
| LBA10 | 0x080–0x1FF | COMPLETE | 跨代无所有者、原样保留/忽略的物理尾部 | 两个独立 EESI 构建的设置函数只覆盖前0x80B并原样回写后0x180B；旧两代没有该尾部写入端 | 读取函数只解密/返回前0x80B，所有已审消费端均忽略后384B | 仓库现行20份唯一金标（19加密+1真实免密）均为384B零；官方写入端的原样保留行为独立闭合 | 完全闭环表示该384B不属于EESI 负载且必须原样保留，不表示协议要求恒零 |
| LBA11 | 0x000–0x003 | COMPLETE | DRKB 魔数 | `CDataSecrity::RandBuffer256` 先写 DRKB | `ReadSector11` 首先校验 DRKB | 22/22 | 完成 |
| LBA11 | 0x004–0x0FF | COMPLETE | random252 | `RandBuffer256`: `srand(time(NULL)); rand()%255` 共252B | `DataEncrypt/DataDecrypt` 将整个 DRKB块纳入 CRC32 密钥输入 | 22/22；均无0xFF；7 CI夹具回归 | 每字节都是密钥扰动材料，来源和消费闭合 |
| LBA11 | 0x100–0x103 | COMPLETE | PDKB 魔数（解密后） | `BuildSector11` 构造 PDKB 明文 | `ReadSector11` 解密后必须校验 PDKB | 22/22 | 完成 |
| LBA11 | 0x104–0x1FF | COMPLETE | 加密的 UID + 全零填充 | 正常注册写入端：`cemsusbregsiter.dll::sub_10014720` 以 `DISK_GEOMETRY_EX.DiskSize` 为 `ullSize`；修复写入端：`UDiskLabelRepair.dll::CLabelRepair::Repair -> sub_10008950(ReWrite11Sector) -> sub_10003A40`，其 `disk_info+0x30/+0x34` 由 `IOCTL_DISK_GET_DRIVE_GEOMETRY(0x70000)` 返回的 `DISK_GEOMETRY` 经 `sub_10019EF0` 64 位乘法计算 `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector` 后生成 LBA11 | 正常消费端：Windows/Linux `ReadSector11` 以精确 DiskSize 解密；修复消费端：`CLabelRepair::Repair -> sub_10008820 -> sub_10003BD0` 用同一 CHS `disk_info+0x30/+0x34` 校验 LBA11，失败才进入 ReWrite11Sector | 严格22份：21/22 精确 DiskSize，1/22 Aigo U335 rev_pmap 为 CHS；另有同一 Aigo rev_pmap 的独立真实精确大小 LBA11 捕获，证明配置类型取决于写入端路径而非硬件；22/22 解密后 UID 正确且 UID 后全零 | 两种已观测盘面配置类型的写入端、消费端、容量算法和实盘均闭合；因此后半252B升级完全闭环 |
| LBA12 | 0x000–0x11F | COMPLETE | 3×96B EDPF | Windows/Linux 写入端；当前 `CEMSUsbRegsiter.dll::CreatePartitions` 的模式1/2/3 均已在隔离 Unicorn 虚拟盘测试框架中原生执行，最终封装密钥调用点分别命中 `0x1003ED77/0x1003ED44/0x1003EDA7` 各1次 | 登录/挂载/兼容链大量消费；模式1=A7F0/A6B0、模式2=SM4-ECB、模式3=AES-128-ECB，统一以 `MD5(effective_password)` 包装16B 文件密钥并以 FileKeyCRC 校验 | 22盘真实设备仍只有模式2；另有三份 **官方二进制虚拟写入端** 512B正向测试夹具，三种模式在同一输入下解出同一16B 文件密钥并命中同一 FileKeyCRC | 主表所有字段生命周期均闭合；虚拟写入端测试夹具明确不计入真实设备统计集，但它是官方一方二进制直接生成的盘面 sample，不是edpcli合成输出 |
| LBA12 | 每条条目 +0x004–+0x007 | COMPLETE | 紧凑布局条目内 `Version` 兼容元数据 | 当前 `CreatePartitions` 对3×96B整表先 `memset(0,0x120)`，后续没有任何 `+0x04` 覆盖写，故当前写入端三条均为0 | Windows 主运行时对96B 条目的登录/改密/挂载消费集中在 PartionType、NeedEncrypt、StartSector、PartionSize、UserKeyCRC、FileKeyCRC、封装密钥、EncryptMode；未发现版本分支。Linux 紧凑布局 `LayoutParsedata/Volume::GetPartitionHeader/PartitionHeader` 只结构缓存整条条目，协议版本由14B 密码信息版本决定，不读取条目内版本 | 已提交原始测试夹具 22/22×3 条目均版本=0；全树22份完整历史备份用当前 edpcli 解密器复算仍 `Version=(0,0,0)` 22/22 | 正式 ABI 字段，但在已覆盖写入端/运行时中是未启用兼容元数据；3×4B=12B 完全闭环。不能再把 `+0x08 PartionCount` 误标为版本，也不禁止未来新写入端配置类型写非零 |
| LBA12 | 0x010–0x013 | COMPLETE | 条目0.NeedDisturb 兼容门禁 | `CUsbRegsiter::CreatePartitions` 写入条目0；Linux `edpdiskglobal.h:82` 定义字段 | `vrvaud_c::NewCheckDisTurbUsb(*)` 回退在 `Format.cpp:0x3CE/0x380` 直接以该 DWORD 非零判成功 | 22/22原始盘=1；20个条目0 type1、2个type2；7 CI夹具锁定 | 完成的是条目0 兼容门控行为；其它条目的 NeedDisturb 不随之升级 |
| LBA12 | 条目1/条目2 +0x010–+0x013 | COMPLETE | 按位置 `NeedDisturb` 兼容元数据 | 当前 `CreatePartitions` 的当前配置类型由整表零初始化后显式形成 `(1,1,0)`：条目0/条目1=1，条目2保留0；字段随96B 条目结构保存 | Windows 行为消费端只命中条目0 回退门禁；UserLogin/挂载参数链不读取条目1/条目2 NeedDisturb。Linux 紧凑布局运行时把 NeedDisturb/NeedEncrypt 一起缓存进 `PartitionHeader`，但后续加解密/文件系统路径无 NeedDisturb 值相关读取 | 已提交原始测试夹具 22/22 为按位置 `(1,1,0)`；全树22份完整历史备份经当前 edpcli 正式解码仍22/22相同 | 8B 闭合为当前按位置兼容配置类型 + 结构性保留 + 跨平台负语义消费端；完全闭环不把条目1/2 解释成条目0 的 MBR 扰动行为 |
| LBA12 | 每条条目 +0x038–+0x047 | COMPLETE | 封装文件密钥材料 | 当前写入端的三条可达分支已经闭合：模式1 `sub_10001190`=A7F0、模式2 `sub_100036E0/sub_10011010`=SM4-ECB、模式3 `sub_1000FC10`=AES-128-ECB；密钥均来自 `MD5(effective_password)`，模式字节写入 `+0x58` | Windows `UserLogin/sub_10028AB0` 对1/2/3分别解包并统一做 FileKeyCRC；模式3 CRC失败还有按模式1重试的历史兼容；Linux当前检查器明确消费模式1/2 | 22盘44条加密真实设备条目全部模式2；隔离执行官方 `CreatePartitions` 又得到模式1/2/3三份确定性512B LBA12正向测试夹具：密码 `ProofPass1!` 三种16B 封装密钥分别经 A6B0、独立标准SM4-ECB、独立标准AES-128-ECB恢复同一 `147196f5a2ec7912edf13f75d766cb42`，三者CRC均=`0xFF4C1D36`且等于盘内 FileKeyCRC；CI锁定 | **一方运行时正向盘面闭环**：没有把虚拟测试夹具冒充物理采集；但官方写入端原生执行、UI→加密→模式可达链、独立读取端往返验证与既有真实设备模式2共同消除了该16B的语义不确定性，因此3×16B从部分闭环升完全闭环 |
| LBA12 | 每条条目 +0x048–+0x057 | COMPLETE | `EncryptFileKey32[16]` 跨代兼容槽 | Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`；当前紧凑布局写入端后续只写 16B 封装密钥 `+0x38..47` 与模式 `+0x58`，所以该16B保持显式零。旧72B `tagEdpPartionInfo` **根本没有**该槽；Linux 检查器的旧版→新版 `GetPartionFromOld` 也只把旧8B 密钥搬到自然对齐 `+0x40`，不填自然对齐 `+0x50 EncryptFileKey32[16]` | 104B 检查器 DWARF正式命名 `EncryptFileKey32[16]@+0x50`，但 `DecryptFileKey/CheckFileKeyCrc/ReadFileSysSector0/DecryptFileSysSector0` 都不读取它；紧凑布局 `libedpedisk.so` 会在按值构造时结构缓存完整96B，但严格按 `PartitionHeader` 符号边界审计，映射到对象 `+0x88/+0x90` 的两个QWORD只在构造器写入，后续没有值相关读取；正对照封装密钥起点对象 `+0x78` 被 SMS4/AES128/OldEdp 解密实际消费。Windows UserLogin/改密同样只消费 `+0x38..47/+0x58` | 严格22份原始盘全部现存 EDPF 条目共66条，`+0x48..57` **66/66全零**；CI `lba12_encrypt_file_key32_compatibility_slots_are_zero_in_original_entries` 锁定 | **LBA12 EncryptFileKey32 兼容槽结构缓存 / 负语义消费端闭环**：旧ABI无槽、新ABI正式保留名字、当前写入端显式零、跨Windows/Linux只结构搬运不参与算法、原始实盘全零。完全闭环表示“兼容槽生命周期/无当前业务语义”闭合，不把它误称保留，也不禁止未来其它ABI结构性携带非零值 |
| LBA12 | 每条条目 +0x059–+0x05F | COMPLETE | 紧凑布局 `Reserved[7]` | Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`，后续只写至 +0x58；Linux DWARF正式字段名 `Reserved[7]` | Windows UserLogin/改密只消费 16B 封装密钥与 +0x58；Linux 解密/改密同样不消费保留 | 22盘66/66 条目全零；CI原始夹具锁定 | **LBA12 打包 `Reserved[7]` 写入端/负消费端闭环**；与前面的 `EncryptFileKey32[16]` 兼容槽分开建模 |
| LBA12 | 0x12A | COMPLETE | 密码信息 `bNoUsbChkPasSafe` / 官方 UI“取消密码复杂性验证” | 与 LBA7 共用同一制标输入链：`pwdComplexityCheckBox.isChecked()` -> `request+0x48` -> `LabelInfo+0x7EC` -> `UsbWriteParam+0x7EC` -> `CreatePartitions` -> `PassInfo+0x0A`；LBA12 构造器保存同一密码信息字节 | `checkdiskback::Update_EDPEDISKSHOWPARAM@0x406B70` 直接比较该字段并生成 SAFE6 显示策略字节 +3；策略经 `CreateSafe6TmpPolicyFile` 加密后被 `EdpEDiskBack` 与 `linuxedpedisk` 两套 `Safe6PolicyFile::GetSafe6Policy` 恢复 | 严格22份18×0+4×1，且22/22与同盘 LBA7 +0x0A相同；CI锁定双值/一致性 | 1B 完全闭环；**1=官方 UI 勾选“取消密码复杂性验证”**，0=未勾选；不再使用含糊“免密安全策略”解释 |
| LBA12 | 0x12C–0x12D | COMPLETE | 未启用密码信息 `ShareBackuppromptPeriod / EncryptBackuppromptPeriod` 兼容字节 | 与 LBA7 共用同一个当前密码信息写入端：完整14B先清零，写 LBA12 前只把版本切换为 `0x0206`，两个周期字节保持0/0；DWARF 正式字段定义同样适用 | Windows 旧版/当前读取端对完整14B结构复制但不消费最后2B；Linux 检查器保存整个密码信息，但实际解密/文件系统检查不读取这两个字段；独立 `vrvaud_c` 备份策略链已排除 | 已提交测试夹具逐盘与 LBA7 两字节一致且均0；历史去重配置类型跨 v0x0064/v0x0206 未见非零 | 与 LBA7 同一未启用兼容字段生命周期闭合；2B 升完全闭环，不推导未实现的时间单位 |
| LBA12 | 0x12E–0x16F | COMPLETE | 表后全零初始化填充 | 写入端整块零初始化且不覆写 | 主读取端不消费该区 | 22/22解密为零 | 写入端+负向消费端+实盘闭合 |
| LBA12 | 0x170–0x1FF | COMPLETE | 表后全零初始化填充 / 连续密文尾部 | Windows 当前 `sub_10014F30` 分配 `sector_size+1` 后整块清零，v0x206 只复制 `0x120+0x0E=0x12E` 结构字节，随后加密整扇；Linux `BuildSector12` 同样先把 `sector_size+1` 全零再只复制表/表尾并整扇加密 | Windows `sub_100160B0` 固定解密0x200B，但只复制 `0x120+0x0E` 返回；Linux主读取端同样只解释表/表尾，不消费表后区 | 严格22份 + 独立SanDisk 解密后 `0x12E..0x1FF` 全零；既有连续密文门禁同时证明 `0x170..` 不是原始尾 | 与 `0x12E..0x16F` 同属一个表后全零填充区；此前部分闭环行是主表陈旧状态，总进度表早已把这144B计入 LBA12 的393B 完全闭环，因此本次只纠账、不重复增加总数 |
<!-- FIELD_LEDGER_END -->

### 4.0 LBA3：EDP 只保留的外部制造/MP 扇区

这一扇区此前只因为 21/22 全零、1 份带 `this is mp mark` 而记为未知。
本轮没有沿用旧文档结论，而是重新从官方写链、官方读取端集合和 22 份原始盘三条线核对。

Windows 当前注册入口
`CUsbRegsiter::RegsiterUsb / sub_1003b560 @ usbregsiter.cpp:0x915..0xA04`
先调用 `ReadSectorData(..., count=0x0D)` 把 LBA0–12 整段读入暂存缓冲区。
SAFE6 分支随后明确重建 LBA4、LBA6、LBA8、LBA11，并由分区/EDPF 辅助函数
处理其它协议扇区；整个函数没有 LBA3 构造器。最后仍以同一个暂存缓冲区
调用 `WriteSectorData(..., count=0x0D)`。所以对 LBA3 而言，官方 EDP 注册写入端
不是“生成零扇区”，而是：

```text
read existing LBA3
    -> no EDP mutation
    -> write the same LBA3 bytes back with the 13-sector batch
```

Linux 当前 `libcemsfilesyscheck.so` 提供
`BuildSector0/4/6/7/8/11/12`、`BuildSector0/1/2_Gpt` 以及
`ReadSector4/6/8/11/12`，独立不存在 `BuildSector3` / `ReadSector3`。
Windows 当前注册、登录和修复组件也未发现 LBA3 负载解析路径。
因此当前 EDP 已经给出 **原样保留 + 不解析** 的明确协议边界。为验证该边界并非
仅当前，本轮又对历史 v19.11.4.1 做了固定 SHA-256 的全 DLL 寻址审计：
31 个 `SetFilePointer` 调用点中，可恢复为 `sector_size × N` 的固定倍率集合精确为
`{1,2,4,6,7,8,12}`，不存在 `N=3`；SAFE6 `virtual_56@0x1000CC50` 也不直接调用
通用 绝对寻址包装函数。可重放脚本为 `scripts/protocol/audit_v19_lba3_preserve.py`。
另有 2021 `UDiskLabelRepair.dll` 的 `RepairSafe6Label/RewriteSafe6BakLabel` 只在
LBA4-LBA12 与尾部镜像间复制9扇区，同样没有把 LBA3 纳入修复负载。

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
历史/真实快照做只读统计集；该扩展集合包含历史/转换状态，只用于配置类型发现，
不改变上述22份严格代际参考的计数。非零 LBA3 只有3份，并形成
**两个不同的 MP 负载配置类型**：2026-08-03 两份逐字节相同的 Kingston 快照
使用 `+0x020..027=a8 82 a4 22 00 20 02 16`；严格 2026-09-03 Kingston 使用
`+0x020..027=b5 7e 9c 45 00 80 00 14`。两类都保持 `+0x001=01` 和
`+0x1F0..1FF="this is mp mark\\0"`，而同型号其它快照还存在整扇全零配置类型。

新增历史配置类型的原始 LBA3 SHA-256 为
`a1e1961d4ab452b6a2f277ee2027c962ea8bed58c6b85f05da12b247a706580e`；仓库夹具
`tests/fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex` 逐字节锁定。
回归 `lba3_mp_marker_has_multiple_real_historical_payload_profiles` 明确要求两个
标记配置类型的中间8B不同，防止把任一单盘的值错升成全局固定模板。

因此不能把 `this is mp mark` 单独建模成 EDP 字段，也不能把21个零样本解释成
“协议规定全零”，更不能把 `+0x020..+0x027` 建模成单一固定常量。相反，跨当前
Windows、Linux、v19.11.4.1 与 2021 修复的证据共同限定了 **EDP 自己对这512B没有
负载所有权：只保留、不解释、不重建**。这与 LBA4 无所有者保留底层字节、LBA5 不透明
原样保留的完全闭环口径一致，因此 LBA3 512B 现按
`manufacturer-owned opaque MP metadata / EDP preserve-only sector` 升为完全闭环。

这里的完全闭环只关闭 **EDP LBA0-LBA12 协议语义**；Phison 精确主机序列化器、
控制器/固件私有字段定义仍是制造商来源开放问题。未来遇到任何未知
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

写入端：

`CLabelManage::BuildSector8 @ diskfile.cpp:805`

```text
EightSecInfo = zero_initialized()
EightSecInfo.Flag = "LLGB"
EightSecInfo.ElabOffset = 0x80
...
copy ELABEL string to byte[0x80]
```

消费端：

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

因此 `LBA8 +0x3E..+0x3F` 2B 从部分闭环升级完全闭环。
其它头字段即便已有官方名称，也不会因为与它相邻而自动升级。

#### LBA8 静态版本/writeTime/保留头

本轮继续对同一个 `tagEdpUsbLableInfo` 逐字段追踪，而不是把整个头一次性
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

消费端侧也重新核对：

- `ReadSector8(char*, UsbLabelParam&)` 解密后只检查 `LLGB`，
  读取 `+0x3E ElabOffset`，随后从该偏移解析 ELABEL；
  不读取 ToolVersion、Labversion、writeTime 或保留；
- `ReadSector8(char*, BYTE*)` 在检查 LLGB 后只是把完整 512B 解密结果
  `memcpy` 给调用者，并返回 `+0x04 cbSize`，同样不解释这些字段。

22 份原始参考重新解密统计：

- ToolVersion：22/22 = `01 00 00 01`；
- Labversion：22/22 = `0x222`；
- writeTime：22/22 非零，且跨独立标签存在多值；
- 保留[64]：22/22 全零。

因此上述 76B 可从部分闭环升完全闭环。相邻的
`HDSerialInfo/MacInfo/UsbOnlyInfo` **没有跟着升级**：当前写入端虽可解释
其当前写法，但 22 盘已经出现历史配置类型差异，尤其
`HDSerialInfo` 非零组和 `UsbOnlyInfo` 空/16位十六进制串并存；旧写入端
尚未闭合。

### 4.2 LBA10 两个 16B 字段：交换区/保密区卷标

旧分析曾把这两个槽解释成“数值/时间戳候选”。重新验证后该解释应废弃。

当前 Windows 读取端 `GetEdpEdiskSetInfo -> sub_1000f930` 在输出结构初始化时：

```text
out + 0x04 = 1
out + 0x08 = "交换区"   // GBK
out + 0x18 = "保密区"   // GBK
```

若 LBA10 存在有效 EESI，则读取端只解密前 `0x80`，并用实盘结构覆盖这份默认值。

写入端 `SetEdpEdiskSetInfo -> sub_1000fc70`：

```text
input->magic = "EESI"
plain80 = input[0x00..0x7F]
cipher80 = A6B0(plain80, CRC32(device_id))
read sector10
replace sector10[0x00..0x7F] = cipher80
write sector10
```

因此 `+0x08/+0x18` 的写入端来源就是 API 调用者提供的两个固定 16B
文本字段，而不是写入端内部再派生的数据。

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

当前构建中汇编可直接看到：

- `info+0x08 -> std::string @ ebp-0x74`；
- type2 分支将 `ebp-0x74.c_str()` 传给 `SetVolumeLabelA`；
- `info+0x18 -> std::string @ ebp-0x54`；
- type4 分支将 `ebp-0x54.c_str()` 传给 `SetVolumeLabelA`。

另一版 `out_raw_data/EdpEDiskCtrl.dll` 也存在同构读取端/写入端与
type2/type4 `SetVolumeLabelA` 路径，排除单版本偶然行为。

> **当前 EESI 闭环：** 19+1 通用统计集的 LBA10 仍是 20/20 全零，说明 EESI
> 不是所有盘都会启用；但仓库另有特定用途物理正向
> `P-EESI-NETAC`，它是修改前只读捕获的真实 Netac OnlyDisk LBA0–12，并由
> `S-EESI-361018` 的官方写入端/消费端链独立闭合。特定用途证据
> 不并入通用统计集计数，但可以为对应配置类型提供物理正例。

当前 LBA10 结论：

- `+0x000..0x07F`：EESI 结构/配置类型，**完全闭环**。官方读取/集合边界、UI
  调用方、标志的值相关消费端、共享/加密标签语义均已闭合；
- `+0x080..0x1FF`：跨代无所有者、原样保留/忽略的物理尾部，
  **完全闭环**；
- `P-EESI-NETAC` 只证明启用配置类型的真实物理存在，不改变 19+1 通用统计集
  的“20/20 LBA10 全零”事实；两者属于不同证据 population；
- 当前状态以 `audit/protocol/byte_ledger.tsv` 为准：LBA10 =
  **512 完全闭环 / 0 部分闭环 / 0 未知**。

历史上曾因通用统计集缺少启用正向而把前0x80记为部分闭环；该状态已被
`P-EESI-NETAC` 和当前 ledger 取代，旧过程只保留在第10节验证历程附录。

### 4.3 LBA9 EETU：时间窗口 + 使用次数 20B 完整闭环

旧分析只观察到 `+0x14 = FFFFFFFF`，一度把它当成未知标志/固定值候选。
重新从 DWARF、Windows 写入端、Linux 消费端和原始实盘四条线交叉后，
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

#### 写入端：Windows `CUsbRegsiter::SetTempUse`

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
- 实际 LBA9 读写辅助函数路径：`usbregsiter.cpp:0x735..0x743`。

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

#### 消费端：Linux `CheckTempUse`

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

- `SetTempUse` 先把魔数之后的 `0x7C` 字节全部清零；
- 随后 `copy(EETU.reverse, request+0x44, 0x66)` 只覆盖 reverse 的前102B，
  即 LBA9 `+0x18..+0x7D`；
- 最后2B `+0x7E..+0x7F` 从未被覆盖，因此始终保留写入端的显式零初始化；
- 对 `BusManageImp::WriteNormalULabel` 的真实机器码做栈区扫描后，上游临时请求
  只显式写开始/终点/useCount；在调用 `SetTempUse` 前没有对
  `tempUse+0x44..+0xA9` 这102B做整体初始化。故前102B即使当前20/20为零，
  也不能解释成协议固定零填充。后续进一步恢复 `sub_1009BAD0` 的真实 thiscall
  参数后，已确认该 memset 清的是 `&var_9EC` 另一对象，完全不覆盖 `&var_BD4`
  SetTempUse 请求，因此这102B的写入端已从“疑似未初始化”闭合为明确的
  写入端未初始化保留底层字节。

Linux `CheckTempUse` 对整个 reverse[104] 都不读取，只消费时间窗和 useCount；
后续 Windows 运行时审计又确认 `ReadTempUseInfo` 整0x80缓存、
`GetTempUseInfo/CheckTmpUse` 的显式读点只到 useCount，`WriteTempUseInfo` 再整0x80
透明写回。因此当前最终结论为：

- `reverse[0..101] / LBA9 +0x18..+0x7D`：**写入端未初始化的不透明保留底层字节**，
  写入端 + 透明保留/负语义消费端 + 真实设备配置类型已闭环，
  后续升级完全闭环；
- `reverse[102..103] / LBA9 +0x7E..+0x7F`：
  **显式全零初始化写入端 + 负消费端 + 20/20 真实设备全零**
  三条证据闭合，升级完全闭环。

本轮因此从部分闭环升级：

- `+0x04..0x0B`：8B；
- `+0x0C..0x13`：8B；
- `+0x14..0x17`：4B；
- `+0x7E..0x7F`：2B；
- 此处记录的是最初阶段累计 **22B 完全闭环**；后续 reverse 前102B补齐写入端/运行时
  透明保存证据后，EETU 本段又新增102B 完全闭环，最终计数以主账本为准。

### 4.4 LBA5：512B 整区是不透明的写保护探测临时扇区

旧分析文档曾把 LBA5 称作“写保护探测牺牲扇区”。这次没有沿用旧结论，
而是从当前/另一版运行时、当前注册写入端和 22 份原始实盘重新验证。

#### 消费端：两版 EdpDiskCtrl 同构

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

也就是说消费端只关心“能否把**同一批字节**写回”，从未解析 LBA5
任何偏移、魔数、标志或校验和。

#### 写入端：注册写入端保留既有 LBA5

`CUsbRegsiter::RegsiterUsb` 的当前写入端流程是：

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
- `BakupUsbSec` 只把当前元数据复制到磁盘尾部备份区，不修改内存 LBA5；
- SAFE1 构造器使用独立临时缓冲区，不存在 `base+5` 写入；
- 对当前与另一版 `EdpDiskCtrl` 的原始扇区引用搜索，`base+5`
  都只出现于上述“读后原样写回”探测。

因此官方写入端对 LBA5 的规则不是“必须写零”，而是**原样保留现有字节**。

#### 22份原始实盘

只读全量复核：

- 22/22：LBA5 512B 全零；
- 22/22：整扇 SHA-256 都是
  `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`；
- 但这只证明当前原始参考的实际内容，不把“零”提升为协议固定值。

因此本账本把整个 LBA5 的 **512B** 标为完全闭环，含义非常具体：

> LBA5 没有字段级负载；整扇内容对协议是不透明字节，注册流程原样保留，
> 运行时仅把它作为可安全执行“读→同字节写回”的写保护探测临时区。

如果未来发现非零原始 LBA5，这个结论不会失效：只要写入端仍原样保留、
探测仍原样写回，非零内容同样符合协议。CI 对当前原始夹具的“全零”断言只用于
防止样本集被悄悄替换，不把全零编码成生成规则。

### 4.5 LBA1/LBA2：GPT 配置类型的一方动态正向证据

旧文档把 LBA1/LBA2 简写成“保留/全零”。重新验证后，这个描述不完整。

Linux 官方库直接保留三套 GPT 构造器：

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

`BuildSector2_Gpt` 则按128B GPT 条目模板写类型 GUID、分区 GUID、
起止 LBA 等字段。

#### Windows 消费端重新验证

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
`sub_1002b2f0`。该解析器的双层循环为：

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

因此 LBA1/LBA2 不是“永远没用的保留零扇区”，而是存在正式 GPT 配置类型。

#### 物理实盘限制与官方二进制虚拟写入端

22份当前原始 SAFE6 参考复核：

- LBA1：22/22 整扇全零；
- LBA2：22/22 整扇全零；
- 两者 SHA-256 均为零扇区
  `076a27c79e5ace2a3d47f9dd2e83e4ff6ea8872b3c2218f66c92b89b55f36560`。

这只能说明当前物理参考盘没有启用 GPT 元数据配置类型，不能反证官方 GPT 构造器。
本轮进一步在**完全隔离、不接触物理原始设备** 的 Unicorn x86-64 环境中直接执行
Linux 一方 `libcemsfilesyscheck.so` 内的三个官方构造器；构造器本体与内部
`calculate_crc32` 均原生执行，只对 libc `memset/memcpy` 做 ABI 边界替代：

```text
BuildSector0_Gpt @ 0x1FD30
BuildSector2_Gpt @ 0x1FFF6
BuildSector1_Gpt @ 0x1FDAA
```

使用 2GiB / 512B 扇区、固定分区 GUID
`00112233445566778899aabbccddeeff`，先由 `BuildSector2_Gpt` 生成条目0，再由
`BuildSector1_Gpt` 对完整16KiB 分区数组计算 CRC 并生成 LBA1。输出为：

- LBA1：`EFI PART`、`Version=0x00010000`、`HeaderSize=92`、`CurrentLBA=1`；
- BackupLBA=4194303、FirstUsable=34、LastUsable=4194270；
- 磁盘 GUID=`a2a0d0ebe5b9334487c068b6b72699c7`；
- 分区条目数组 LBA=2、条目数=128、条目大小=128；
- 独立 IEEE CRC32 复算：头=`0xA4B46C72`、数组=`0xD32CFEA7`，均与官方写入端输出一致；
- LBA1 `+0x5C..+0x1FF` 全部来自512B官方模板的显式零区；
- LBA2 条目0：Basic Data 类型 GUID、调用方提供的分区 GUID、起点=63、
  终点=4194270、属性=0、名称[72]=0。

随后把同一34扇暂存镜像直接喂给当前 Windows
`CEMSUsbRegsiter.dll::IsAllowRegisterCommonLabel/sub_1002AB70`，该官方消费端
原生执行后返回 **2 = GPT**。所以 LBA1 已具备跨平台
一方写入端→盘面→消费端正向闭环。

这里仍保留一个严格区分：`BuildSector2_Gpt` 每次只负责**完整写一个128B 有效条目**，
它不清理其它条目槽；所以暂存 area 的 entries1..127 零值仍不能冒充官方
写入端常量。但后续沿消费端继续追踪后，未使用条目的残留已经按**不依赖具体值的
无所有者存储**闭合：当前 Windows `sub_1002B2F0` 和 Linux
`AnalyzeGptPartitionTable@0xFB36` 都先比较16B TypeGUID，只有匹配受支持非零GUID后才读取
起点/终点/属性等残留内容；Windows 官方解析器对 `TypeGUID=0 + residual=0xA5` 的
动态探测又证明336B 残留运行时读取次数为0。

因此最终状态为：

- LBA1：**512B 完全闭环**；
- LBA2 `0x000..0x07F` 条目0：**128B 完全闭环**；
- LBA2 条目1..3：16B TypeGUID 由单分区创建器 + 未使用判别项闭合，
  其余112B/条目由一方负向消费端闭合为 **未使用条目无所有者残留**；
- LBA2：**512B 完全闭环**。

测试夹具 `official_virtual_gpt_lba1.hex` / `official_virtual_gpt_lba2.hex` 与 CI
`official_virtual_gpt_builder_emits_valid_lba1_and_entry0` 固定有效写入端结构与 CRC；
`strict_progress_has_no_partial_detail_rows_for_fully_complete_lbas` 则防止以后把 LBA2 残留
重新误记为写入端负责全零或部分闭环。

## 5. LBA11 完整写入端 / 消费端追踪

### 5.1 写入端：Linux 官方实现

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

### 5.2 消费端：Linux 官方实现

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

### 5.3 Windows 独立写入端 / 消费端

Windows `cemsusbregsiter.dll` 存在与 Linux 完全独立、但公式一致的实现：

| 角色 | 函数 | 地址 | 反编译位置 |
|---|---|---:|---:|
| LBA11 构造器 | `sub_10014720` | `0x10014720` | `cemsusbregsiter.dll.m` 约 L78343 |
| DRKB/随机写入端 | `sub_10002B90` | `0x10002B90` | 约 L82081 |
| KDF/加密 | `sub_10002C30` | `0x10002C30` | 约 L82112 |
| LBA11 读取端 | `sub_10015F00` | `0x10015F00` | 约 L93391 |

关键证据：

- `sub_10002B90` 固定写 `0x424B5244 == "DRKB"`，随后循环生成
  `rand()%0xFF`；
- `sub_10014720` 构造 `0x424B4450 == "PDKB"`，并把 UID C 字符串
  写到明文 `+0x04`；
- `sub_10002C30` 把
  `DRKB256 || VID4 || PID4 || size8` 作为 CRC32 输入，再用 CRC
  的 4B 小端作为后半 256B 加密密钥；
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

### 5.4 Windows 当前写入端的容量来源

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

因此**当前 Windows 写入端的 LBA11 KDF 容量输入就是物理
`DISK_GEOMETRY_EX.DiskSize`**，不是 CHS 推导值。

此前唯一缺口是 Aigo U335 `rev_pmap / onlyid=1987718388` 为什么使用 CHS 容量。
本轮已从独立官方修复组件闭合这条路径：

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

`sub_10019EF0` 的机器码已复核为标准 64 位乘法辅助函数：返回 `EDX:EAX`，
不是业务函数。对常见 255/63/512 几何，这正是此前实盘复算得到的 CHS-floor
容量。也就是说，**CHS 不是注册写入端的隐式分支，而是修复写入端/读取端的
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

同一 Aigo U335 `rev_pmap` 的辅助真实采集同时存在 CHS 与精确 DiskSize 两种
LBA11，进一步证明差异来自写入端路径，而不是硬件身份本身。至此两种已观测
配置类型都具备写入端、消费端、容量算法和真实盘验证，因此：

- LBA11 `0x000..0x0FF`：完全闭环；
- `0x100..0x103`：完全闭环；
- `0x104..0x1FF`：由部分闭环升级完全闭环；
- **LBA11 整扇 512B = 100% 完全闭环**。

## 6. LBA7 / LBA12 EDPF 字段写入端/消费端图

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

但 **Windows 真实物理 LBA7 不是这个 72B 自然对齐 ABI**，而是去掉对齐洞后的
64B 紧凑布局 ABI：

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
+0x38 EncryptFileKey     (8B legacy 封装密钥)
```

**LBA7 打包 64 字节 ABI 与 Linux 自然对齐 72 字节 ABI** 已由三条独立证据锁定：

- `libcemsfilesyscheck.so::BuildSector7@0x1DCDA` 的自然对齐构建复制
  `0xD8 = 3*0x48`，其密码信息位于 `+0xD8`；
- Windows `cemsusbregsiter.dll::sub_10016490` 明确按 `0x40` 读取旧表并逐字段
  扩展到 `0x60` 运行时表；`edpediskctrl.dll::sub_100125B0` 做反向
  `0x60 -> 0x40` 映射；
- 22份原始真实设备：按 `0x40` 步长，21/22 为3条 EDPF、1份已知
  中间态为2条，且 `+0xC0` 的14B 密码信息 22/22 可恢复合法版本；
  按 `0x48` 步长则 22/22 都无法得到三条连续 EDPF，`+0xD8` 也无一得到合法
  密码信息。CI 原始夹具另外锁死 `0x40` 三条条目与 `+0xC0` 表尾。

这里的“22份”继续特指**原始生成协议参考集**。另有
`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4`：
它是 2026-08-23 从真实 SanDisk Ultra 免密码 U 盘只读采集的设备快照，
不是 edpcli 自生成/免密转换产物。本轮把它作为独立第23份真实行为/配置类型
样本纳入复核，但不拿它替代22份原始生成参考。该盘 LBA7 明确为：

```text
entry0: Version=0, PartionCount=2, PartionType=2, NeedDisturb=1, NeedEncrypt=1
entry1: Version=0, PartionCount=2, PartionType=4, NeedDisturb=1, NeedEncrypt=1
pass-info Version=0x0064
```

因此真实产品确实存在两条条目的 LBA7 配置类型；此前22份参考中的 Netac
`onlyid=949028302 @ 17:24:33` 仍因同 onlyid 前后只有 LBA7 被改动而作为
该扇区的局部实验态降权，但不能再把“LBA7=2”本身视为实验态特征。
此外，`no_password_disk4/info/disk4_info.json` 中旧解析结果
`"ver": 2` 是把 `PartionCount@+0x08` 错标成版本；按物理紧凑布局 ABI
重新解码后两个条目的 `Version@+0x04` 都为0。仓库
`protocol_evidence/sandisk_ultra_authentic_no_password_lba7.hex` 回归门禁
专门拦截这类字段错位。该新增样本不改变当前完全闭环/部分闭环字节计数。

Linux 写入端/读取端原源码位置：

- `CLabelManage::BuildSector7` → `diskfile.cpp:895`；
- `CLabelManage::BuildSector12` → `diskfile.cpp:921`；
- `CLabelManage::ReadSector12` → `diskfile.cpp:1195`。

Windows 当前写入端：

- `cemsusbregsiter.dll::CUsbRegsiter::CreatePartitions`
  （反编译 `sub_1003db50`，本地约 L120709）；
- 日志保留的原源码位置：
  `usbregsiter.cpp:0xE13..0xE15`；
- 该函数按 `0x60` 步长构造紧凑布局 LBA12 条目。

### 5.9 官方“启动区与交换区二合一”制盘链：UI → 请求 → EDPF → MBR → FormatDisk

本轮继续从一方制标程序反向追踪真实免密码 SanDisk 的来源，已经把
“启动区与交换区二合一”从产品 UI 一直闭合到物理 MBR。这里要特别区分：

- **产品术语**：“启动区与交换区二合一”；
- **EDPF 物理布局**：`type2 + type4` 两条条目；
- **操作系统可见布局**：一个从 LBA63 开始的普通 MBR `0x07` 分区；
- **当前 FormatDisk 行为**：把该前部卷按“交换区”特殊配置类型格式化，
  不再走普通独立 type1 启动区的文件部署分支。

#### 5.9.1 UI 配置类型编号不是推断：四个单选项直接写 `part=0/1/2/3`

`cemssafeudisklabeltool.exe` 的 UI 对象名明确包含：

`defaultPartRadio`、`bootAndExchangePartRadio`、`allEncryptPartRadio`、
`inAndOutNetPartRadio`。

`WriteLabel` 的 Qt 元调用表和对应机器码已经把四个单选项与请求值固定为：

```text
part=0  defaultPartRadio
        默认三分区

part=1  bootAndExchangePartRadio
        启动区与交换区二合一

part=2  allEncryptPartRadio
        整盘加密

part=3  inAndOutNetPartRadio
        内外网通用双分区
```

请求结构中对应关系也已经闭合：

```text
normalDetail +0x34 -> part
normalDetail +0x38 -> boot MiB
normalDetail +0x3C -> exchange MiB
normalDetail +0x40 -> encrypt MiB
normalDetail +0x44 -> encrypt mode
normalDetail +0x49 -> noPwd
```

这里的 UI/请求逻辑不是单一副本偶然值。本机三份 LabelTool：

```text
cemssafeudisklabeltool.exe
cemssafeudisklabeltool_orig.exe
cemssafeudisklabeltool_2ndbackup.exe
```

虽然整文件和整 `.text` SHA-256 不同，但三段关键代码
`ValueChangedSlot@0x466580`、请求构造器 `@0x467470`、
`PartType UI@0x46F2C0` 的局部机器码逐字节一致。三份 PE 的 compile time/GUID
也一致为 `2024-01-07 03:22:07` / `9AD6E70914B54620ACEFF60ACD36BD811`。
因此本小节对 UI 公式按“同一官方构建族的三个本地副本”计证据，而不是把一个修改副本
外推成历史协议恒等式。

#### 5.9.2 容量公式闭合：二合一的大 type2 直接来自交换区 MiB

Qt 元调用已把容量相关槽精确映射为：

```text
ValueChangedSlot(int)                         -> sub_466580
PartTypeRadioChangeSlot()                     -> sub_4714b0
exchangePartSizeEdit()                        -> sub_471dc0
encryptPartSizeEdit()                         -> sub_471f30
on_lineEditExchangeSize_editingFinished()     -> sub_466be0
on_lineEditEncryptSize_editingFinished()      -> sub_466d40
```

`PartTypeRadioChangeSlot -> sub_46F2C0` 对 `part=0` 和 `part=1`
都使用同一 50:50 初值。令：

```text
total_mib = 当前设备可用于分区的容量 MiB
boot_mib  = 固定 Boot 保留 MiB（来自全局 label 参数）
available = total_mib - boot_mib
p         = slider 位置，百分数
```

则普通/二合一可调模式的核心公式为：

```text
exchange_mib = available * p / 100
encrypt_mib  = total_mib - exchange_mib - boot_mib
```

初始 `p=50`，滑块范围=`1..99`。用户直接编辑交换区或加密
输入框时，另一边仍按同一恒等式补齐；两个 `editingFinished` 又把手填 MiB
反算成滑块百分比：

```text
p_exchange = exchange_mib * 100 / available
p_encrypt  = 100 - encrypt_mib * 100 / available
```

底层 `cemsusbregsiter.dll::sub_10046D20` 再明确规定：

```text
type1 -> object+0x7DC (boot MiB)     * 0x100000
type2 -> object+0x7E0 (exchange MiB) * 0x100000
type4 -> object+0x7E4 (encrypt MiB)  * 0x100000
```

所以二合一的“前部大区”不是隐藏执行
`boot + exchange` 的第二套容量公式；其 EDPF 条目本身就是 **type2**，
大小直接由交换区 MiB 字段生成。产品名称里的“启动+交换二合一”描述的是
用途/配置类型，不是“保留 type1 条目后把两段物理相加”。

#### 5.9.3 `part=1` 的官方 EDPF 构造：严格只有 type2 + type4

底层布局构造函数按 `part` 做显式分支选择：

```text
part=0:
    PartionCount = 3
    entry0 = type1
    entry1 = type2
    entry2 = type4

part=1:
    PartionCount = 2
    entry0 = type2
    entry1 = type4

part=2:
    PartionCount = 2
    entry0 = type1
    entry1 = type4
    # current writer 另有整盘加密几何修正

part=3:
    PartionCount = 2
    entry0 = type1
    entry1 = type2
```

因此真实免密码 SanDisk 的 `PartionCount=2 + type2/type4` 已不再只是
“看起来像某种免密配置类型”，而是和一方 `part=1` 构造分支逐项一致。

#### 5.9.4 当前 CreatePartitions 几何：条目0 从 LBA63 开始，type4 紧随其后

`CUsbRegsiter::CreatePartitions/sub_1003DB50` 的当前几何计算可归纳为：

1. 扇区大小取设备真实 `SectorSize`；
2. 第一个有效分区固定从 `StartSector=63` 开始；
3. 第一段有效字节数 = 第一段边界字节数 - `63 * SectorSize`；
4. 第二条条目的 StartSector = 第一段边界 / SectorSize；
5. 后续分区按前一分区边界连续排列；
6. 盘尾仍预留当前写入端的元数据/保留区，不全部分配给 type4。

真实 SanDisk Ultra 给出精确物理交叉验证：

```text
MBR visible partition:
type  = 0x07
start = 63
count = 117611802 sectors

LBA12 entry0 (type2):
StartSector  = 63
PartionSize  = 117611802 * 512

LBA12 entry1 (type4):
StartSector  = 117611865
```

并且：

```text
63 + 117611802 = 117611865
```

即 MBR 可见区、LBA12 type2 和 type4 起点在扇区级完全连续，不存在独立 type1
夹在中间。

#### 5.9.5 MBR 是“免登录直接访问”的关键：type2 位于条目0 时官方返回 0x07

`RegsiterUsb` 在 EDPF 构造之后会调用 MBR 构造器。分区类型选择函数
`sub_10041A80` 会查询 EDPF 中 type2/type1/type4 的位置；当 **type2 存在且位于
条目0** 时直接返回 `0x07`。

随后 `sub_10013E40` 的机器码明确构造标准 MBR：

```text
partition_entry[0].type      = caller supplied type   # part=1 -> 0x07
partition_entry[0].start_lba = 63
partition_entry[0].count     = calculated sector count

partition_entry[1..3] = zero
MBR[0x1FE] = 0x55
MBR[0x1FF] = 0xAA
```

因此当前官方二合一链是：

```text
bootAndExchangePartRadio
        -> part=1
        -> EDPF: type2 + type4
        -> type2 is entry0
        -> MBR selector returns 0x07
        -> MBR exposes type2 @ LBA63
        -> OS native filesystem mount
```

这解释了真实免密码盘为什么无需先走
`EdpEDisk.exe -> UserLogin -> unwrap file key -> EdpMountFile` 才能访问前部卷。

同时必须保留一个容易误判的事实：真实 SanDisk 的 type2
`NeedEncrypt=1`，但原始前部卷已验证为**明文 NTFS**。二者不矛盾：
`NeedEncrypt` 是 EDP 登录/挂载链的字段；二合一前部卷在日常访问时由标准 MBR
直接暴露给操作系统，绕过该登录挂载链。因此不能把“EDPF NeedEncrypt=1”机械解释为
“MBR 直接访问时物理扇区必然是密文”。

#### 5.9.6 当前 FormatDisk 对 type2+type4 有专用分支；产品“二合一”不等于复制 type1 内容

`CUsbRegsiter::FormatDisk` 直接识别：

```text
PartionCount == 2
entry0.PartionType == 2
entry1.PartionType == 4
```

命中后设置专用状态。二进制中的 GBK 常量进一步区分：

```text
BD BB BB BB C7 F8 -> “交换区”
C6 F4 B6 AF C7 F8 -> “启动区”
```

当前 `FormatDisk` 对上述二合一配置类型将前部可见卷走“交换区”分支；
普通存在独立 type1 的路径才走“启动区”逻辑，并在相应条件下部署
`EdpEDisk.exe`、`EdpDisk.chm`、`DiskDigger.exe` 和皮肤资源。

因此应撤销“二合一就是把 type1 的启动文件完整搬进 type2”的简单解释。
当前一方代码支持的更精确描述是：

> **取消独立 type1，把前部大卷作为 type2/交换区直接暴露；产品层把这种用途组合称为
> ‘启动区与交换区二合一’，但当前 FormatDisk 并不机械复制普通 type1 的完整启动区部署步骤。**

这里存在明确的历史边界：本机目前只有当前 `cemsusbregsiter.dll` 能完整追到
`CreatePartitions/FormatDisk`；尚未取得一个可独立执行、能证明旧年代
“二合一”是否部署不同启动文件集的历史 FormatDisk 写入端。
因此**不得**把当前的文件部署差异升级为所有历史版本协议恒等式。

#### 5.9.7 LBA7 与 LBA12 在二合一盘上不是“完全重复的两张几何表”

真实 SanDisk 还暴露出一个容易误读的兼容层：

- LBA12 当前 96B 条目给出实际连续 type2/type4 几何；
- 旧 LBA7 紧凑布局表同样只有 type2/type4 两条，但 type4 的旧版几何可落到
  盘尾兼容/占位位置，真实样本中其旧表大小只有 `0xC00 = 3072B`；
- 当前 `CreatePartitions` 先组织旧格式表，再生成/修正当前 LBA12 表。

因此兼容实现不能简单要求“LBA7 type4 StartSector/PartionSize 必须逐字节等于 LBA12”。
对当前挂载/实际分区几何，应优先服从已经由 Windows/Linux 消费端闭合的
LBA12 96B 紧凑布局运行时表；LBA7 继续按旧版 ABI/配置类型单独解释。

综合本轮证据，真实免密码 SanDisk 的官方制盘原理可以收敛为：

```text
官方 UI part=1
  -> type2 + type4
  -> type2 从 LBA63 开始，占据保密区之前的前部大区
  -> MBR 为同一 type2 物理范围建立标准 0x07 分区
  -> OS 可直接挂载前部明文文件系统，无需 EDP UserLogin
  -> type4 仍保存密码/file-key/EncryptMode 等 EDP 安全元数据并走安全挂载链
```

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

### 6.0 条目版本与条目1/条目2 NeedDisturb：兼容元数据生命周期闭合

本轮直接验证 Windows 官方 PE 机器码：

- \`cemsusbregsiter.dll::sub_10016490\` 按 \`0x40 -> 0x60\` 遍历三条旧版条目，
  明确复制 \`Version@+0x04\` 与 \`NeedDisturb@+0x10\`；
- \`edpediskctrl.dll::sub_100125B0\` 的 \`0x60 -> 0x40\` 反向转换也逐条复制二者；
- 因此它们确实属于物理旧版 ABI 字段，而不是填充。

当前写入端 \`CUsbRegsiter::CreatePartitions/sub_1003DB50\` 的真实赋值序列是：

\`\`\`文本
memset(old_table, 0, 3 * 0x40)

# 条目版本
条目0.版本 = 0        # 三条都没有 +0x04 覆盖写
条目1.版本 = 0
条目2.版本 = 0

# NeedDisturb
调用方传入 need_disturb = 1
条目0.NeedDisturb = need_disturb
条目1.NeedDisturb = need_disturb
条目2.NeedDisturb = 0    # 没有覆盖写，保留 memset
\`\`\`

这与原始三分区实盘的 \`1/1/0\` 完全一致，也解释了为何不能把 NeedDisturb
理解成 \`PartionType\` 的函数：新增真实免密 SanDisk 的两条表中，
type4 位于条目1，因此其 NeedDisturb=1；标准三分区 type4 位于条目2，
NeedDisturb=0。回归测试
\`lba7_need_disturb_is_not_a_partition_type_invariant\`
固定这一配置类型/位置差异。

消费端 / 结构性保留继续向下追踪后的边界：

- 两版官方 \`vrvaud_c\` 都把完整紧凑布局旧版表读入缓冲；
- \`NewCheckDisTurbUsb\` / \`NewCheckDisTurbUsbEx\` 实际只检查
  条目0 \`NeedDisturb@+0x10\`；
- 本轮把两版全局表的 ABI 又按机器码/地址重新锁定：ydcc 构建
  `0x1020BF40`、Win10 构建 `0x10172520` 都先 memset **0xC0**，随后承接
  3×0x40 紧凑布局旧版表；两版 `sub_*C5D0` 都以 `(i << 6)+base+0x0C`
  遍历 PartionType，明确步长=0x40，不是0x60 运行时；
- 对这两个0xC0全局区做完整静态交叉引用：两版都只有
  `entry0 NeedDisturb@base+0x10` 的行为读取；条目1/条目2 `+0x10`
  和三条 `Version@+0x04` 均无直接交叉引用。动态循环也只读取 `PartionType@+0x0C`；
- Linux \`CLabelManage::GetPartionFromOld\` 对版本/NeedDisturb 只是 ABI 搬运；
- Linux \`PartitionHeader\` 体系会携带整条新版条目；进一步对
  \`CDiskReader::GetTagPartitionInfo\`、\`DecryptFileKey\`、
  \`CheckFileKeyCrc\`、\`ReadFileSysSector0\`、
  \`DecryptFileSysSector0\` 的真实机器码逐一复核后，消费字段集中在
  标志/PartionType/UserKeyCRC、StartSector、FileKeyCRC、封装密钥、EncryptMode，
  均不读取条目 \`Version@+0x04\` 或 \`NeedDisturb@+0x10\`；
- 旧 Windows \`EdpEDiskCtrl\` 读取旧版 LBA7 后，实际协议代际仍由
  14B 密码信息版本决定，不依赖条目 \`Version@+0x04\`。

实盘方面，已提交原始测试夹具的全部有效条目与新增真实免密 SanDisk
两条条目均为 \`Version@+0x04=0\`；扩展只读历史扫描也没有发现非零版本。
NeedDisturb 则稳定按**位置配置类型**出现：

- 三条条目：\`(1,1,0)\`；
- 两条条目：\`(1,1)\`。

尤其 SanDisk 的 type4 位于条目1 且 NeedDisturb=1，而标准三分区 type4 位于
条目2 且 NeedDisturb=0，因此它不能被解释成 `PartionType -> NeedDisturb`
恒等映射。回归门禁现在对所有已提交原始测试夹具逐条锁定
版本=0 与上述按位置 NeedDisturb 配置类型，而不是只抽两个代表盘。

这里的完全闭环语义必须与条目0 的真实 MBR 行为分开：

- **3×版本共12B**：正式 ABI 兼容元数据。当前写入端由整表
  全零初始化产生0；Windows 旧版/新版转换器双向结构性保留；
  Windows 两个独立构建与 Linux 文件系统检查链均不把它作为版本选择或行为条件，
  真正协议代际由密码信息版本决定；
- **条目1/条目2 NeedDisturb 共8B**：当前写入端的按位置
  兼容元数据。条目1继承调用者参数1，条目2继承全零初始化 0；
  转换器结构性保留；已审 Windows/Linux 运行时没有条目1/2
  行为读取。只有条目0 的同名 DWORD 另有 MBR 扰动/去扰行为消费端。

因此这20B现在满足与 `EncryptFileKey32` 兼容槽相同的严格闭环模型：
**明确写入端/配置类型 + 双向/跨ABI 结构性原样保留 + 跨平台
负语义消费端 + 多真实配置类型实盘门禁**。它们不是保留，
也不能被清洗为统一常量；未来若发现其它写入端配置类型，应扩展配置类型而不是
推翻“当前运行时只结构保留、不赋予独立业务语义”的闭环。

本轮先新增 **20B 部分闭环 -> 完全闭环**；随后密码信息
\`+0x0C/+0x0D\` 两个 BackupPromptPeriod 兼容字节也由独立跨版本
读取端/写入端审计闭合，见下节，因此 LBA7 最终达到 **512/512 完全闭环**。

#### LBA7/LBA12 BackupPromptPeriod：正式但未启用的兼容字节

Linux DWARF 把这两个字节直接钉到 \`edpdiskglobal.h:164/165\`：

- \`ShareBackuppromptPeriod\`：\`BYTE @ +0x0C\`；
- \`EncryptBackuppromptPeriod\`：\`BYTE @ +0x0D\`。

它们不是填充/位域。当前 Windows 写入端
\`CreatePartitions/sub_1003DB50\` 先把完整14B 密码信息显式清零，随后有效
存储最远只到 \`+0x0A\`，因此两字节的当前写入端是明确的0/0。
生成 LBA12 时只把同一密码信息的版本改为 \`0x0206\`，两字节不变。

读取端侧继续跨四个不同哈希的 \`EdpEDiskCtrl.dll\` 版本复核：

- 当前 ydcc 与 out_raw 两版都把完整14B 密码信息结构性复制到输出；
  随后只对 \`Version(+0)\`、共享重试 \`(+3)\`、加密重试 \`(+6)\`
  做 XOR/字段处理；
- 两个更老构建同样输出完整14B；Win10 构建的成功尾部可直接看到
  3×DWORD + 最后1×字的完整结构搬运，最后字正是 \`+0x0C/+0x0D\`，
  随后仍只处理 \`+0/+3/+6\`；
- Linux \`libcemsfilesyscheck.so\` 保存完整密码信息，但实际文件系统检查链
  不读取这两个字节。

另外，两代 \`vrvaud_c\` 虽有
\`BackupPromptInfo/BackupStartTime/BackupEndTime\`，机器码已经证明它们从
策略字符串解析到独立字符串/DWORD 全局变量；其磁盘旧版表全局严格是
\`0xC0 = 3×0x40\` 紧凑布局条目，不包含后续14B 密码信息。该链与
密码信息没有数据流，不能凭名字相似合并。

实盘方面：

- 已提交原始样本的 LBA7/LBA12 两份副本逐盘均0/0且完全一致；
- 全目录只读去重扫描19个真实 LBA7 密文配置类型，覆盖密码信息
  \`v0x0064\` 与 \`v0x0206\`，仍是19/19=0/0；
- 没有任何非零历史配置类型。

因此这2B/每份表的完全闭环含义不是“周期的单位已证明为小时或天”，而是：
**正式命名的备份提示周期兼容字段在所有已覆盖产品实现中
处于未启用状态；写入端明确写0，读取端只结构保存/返回而无值相关语义消费。**
未来如果发现非零旧配置类型，必须原样保留/报告并扩展配置类型，不能按当前
规则机械清零，也不能把未知非零值强行解释成当前不存在的时间单位。

### 6.1 条目0 NeedDisturb：4B 已完整闭合

写入端：

- Windows `CreatePartitions` 生成紧凑布局条目；
- Linux 官方字段名为 `NeedDisturb @ +0x10`。

消费端：

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
- 20/22：条目0 类型=1；
- 2/22：条目0 类型=2；
- 22/22 密码信息版本=`0x0206`。

因此这 **4B** 可升级完全闭环。注意完成的是“旧兼容识别路径的非零门控”
这一具体行为，不是对字段名作“扰码/防篡改”等词义扩张；条目1/条目2
仍需分别追消费端，不能因为同名字段而自动升级。

### 6.2 LBA7 v0x0064 +0x38..+0x3F：8B 旧版文件密钥封装完整闭合

Windows 运行时 `edpediskctrl.dll::sub_10026050` 是旧表密钥的直接消费端和
改密写入端。对目标条目，它依据密码信息/版本决定密钥长度：

```text
key_len = 8
if pass_info.Version == 0x0206:
    key_len = 16

plain_key = unwrap(password, entry.EncryptMode, entry.wrapped_key)
if CRC32_bare(plain_key[0:key_len]) != entry.FileKeyCRC:
    reject
```

对 v0x0064 旧版表，转换后的 `EncryptMode=0`。`sub_10028AB0` 的 mode0
分支调用 `sub_10011290 + sub_10011450`。继续拆机器码可得：

- `sub_10011290(password)`：按小端 4B chunk 求和；不足4B的尾部
  以零补齐后再加，结果按 u32 回绕；
- `sub_1005CEF0` 是无符号 64-位右移辅助函数；
- `sub_1004EF80` 是无符号 64 位乘法辅助函数；
- 化简编译器辅助函数后，`sub_10011450` 对两个32位半字的实际变换为：

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

22份原始真实设备的只读独立复算覆盖全部非零旧版加密条目：

- 共 **28条** type2/type4 条目同时具有非零 `FileKeyCRC + wrapped8`；
- 28/28 的 `UserKeyCRC == 0x0429735D`；
- 每条分别执行 `wrapped_lo/hi XOR 0x91919191` 得到8B 文件密钥；
- **28/28** 都满足
  `CRC32_bare(unwrapped_file_key8) == entry.FileKeyCRC`；
- 当前风格对应字段为零，与 newer 配置类型分界一致。

因此每条紧凑布局条目的 `+0x38..+0x3F` 8B 可由部分闭环升为完全闭环，
三个条目共新增 **24B 完全闭环**。`+0x34..+0x37 FileKeyCRC` 早已因 CRC
消费端链计入完全闭环，本轮不重复增加4B/条目。

CI 回归分别锁定物理 LBA7 必须使用0x40 紧凑布局步长，以及已提交原始
测试夹具的默认密码旧版 wrapped8 必须能按上述算法解包并通过 FileKeyCRC。

### 6.2.1 LBA7 `0x0CE..0x1FF`：306B 表后全零区域完整闭合

Windows 主物理旧版表写入端 `edpediskctrl.dll::sub_10010FC0` 已回到实际实现复核：

```text
staging[0..0xFFF] = 0                         # sub_1004D110 == memset
staging[0x000..0x0BF] = packed_table[0xC0]
staging[0x0C0..0x0CD] = pass_info[0x0E]
rolling_xor(staging[0x000..0x1FF])
WriteFile(LBA7, 0x200)
```

`sub_1004D110` 不是根据命名猜测：其机器码按 `arg2` 长度逐字节/`rep stosd`
填充 `arg1`，语义就是 memset。`sub_10010FC0` 在任何表/尾部复制之前把
暂存清零，并且最后一个结构写入恰好结束在 `0x0CE`，所以明文
`0x0CE..0x1FF` 的306B有明确的 **写入端负责的全零写入端**。

对应 Windows 消费端 `ReadPartionInfoExEx/sub_10010B40`：

1. 从 LBA7 读取完整扇区；
2. 对前512B执行完整滚动解码；
3. 魔数合法后只向调用者复制 `decoded[0x000..0x0BF]` 的0xC0 表；
4. 再复制 `decoded[0x0C0..0x0CD]` 的0x0E 密码信息；
5. **从不返回或解释 `decoded[0x0CE..0x1FF]`。**

Linux `BuildSector7@0x1DCDA` 也独立执行“约2KB 暂存先清零→复制自然对齐
3×0x48 表 +0x0E 尾部→对完整512B 滚动”。因为 Linux 自然对齐 ABI 的尾端是
`0x0E6`，这条证据只用于确认同源构造器的未用区域零初始化原则，**不能**把
Linux `0x0E6` 偏移混成 Windows 紧凑布局 `0x0CE` 的物理边界。

严格22份原始代际参考集再逐盘独立复算：

- 22/22 LBA7 均按各自 `CRC32(device_id)` 派生滚动密钥恢复 `EDPF`；
- **22/22 `decoded[0x0CE..0x200] == zero[306]`**；
- 独立 SanDisk 原始同样为完整306B零，无旧版非零反例。

新增 CI 门禁 `lba7_post_table_plaintext_is_zero_through_sector_end`。因此这306B
满足字段/区域边界、官方写入端、负向消费端、原始实盘四证合一，
由 **未知 -> 完全闭环**。LBA7 严格状态随之变为：

```text
512 COMPLETE / 0 PARTIAL / 0 UNKNOWN = 100.0%
```

表后审计当时新增306B 完全闭环；后续又闭合密码信息
`bNoUsbChkPasSafe(+0x0A)` 1B。本轮再按兼容元数据生命周期闭合
3×版本共12B与条目1/条目2 NeedDisturb 共8B，最后把两个
BackupPromptPeriod 字节闭合为未启用兼容字段；LBA7 至此整扇完成。

### 6.3 LBA12 +0x38..+0x47：历史阶段记录（当前模式1/2/3 语义均已完全闭环）

> 本节保留当时仅闭合模式2 的推导过程；**当前结论以前方标准字段账本为准**。后续一方 `CreatePartitions` 隔离执行已经补齐模式1/模式3 正向盘面与独立 unwrap/CRC，因此整字段现为语义完全闭环；模式1/模式3 仍在 `profile_coverage.tsv` 标记为 `MISSING_PHYSICAL`。

当时对紧凑布局条目的 16B 封装文件密钥做了重新独立审计，
不再沿用更早脚本结论。

Windows 写入端：

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

其中 **LBA12 v0x0206 隐藏的默认密码文件密钥封装** 的
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
已验证 A6B0 算法重算得到。用相反的 A7F0 方向时索引会越界，
也提供了负向验证。

Windows `sub_100036e0` 所走的模式2 算法可由以下常量/轮函数确认
为标准 SM4：

- FK =
  `A3B1BAC6 56AA3350 677D9197 B27022DC`；
- CK 从 `00070E15 1C232A31 383F464D ...` 顺序展开；
- S-box 与标准 SM4 S-box 逐字节一致；
- 密钥序列 T' 使用 rot13/rot23；
- 轮 T 使用 rot2/rot10/rot18/rot24；
- 总计32轮。

Linux 消费端：

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

22份原始真实设备参考集的只读重算：

- 共 66 条紧凑布局条目；
- 44 条 type2/type4 为 EncryptMode=2；
- 其中 43 条 `UserKeyCRC=0x0429735D`；
- 使用 OpenSSL 3.6.3 标准 SM4-ECB +
  `MD5("LtSWi[2f)j")` 独立解包：**43/43** FileKeyCRC 校验通过；
- 唯一非默认 type2 的同盘 type4 仍是默认密码；
  从 type4 独立解出共享文件密钥
  `eadd58009f9abe0625a1f1f779d4c98b`，
  CRC=`F7EEA980`，同时精确匹配 type2/type4 保存的 FileKeyCRC；
- 两条 16B 封装密钥不同，排除“直接复制同一封装材料”。

所以当前实盘使用的
`v0x0206 + mode2 + oldSM4!="1"` 分支已经闭环。

随后继续追完 **LBA12 备用封装模式算法映射** 后，
可以把“其它算法分支未知”这一缺口进一步消掉：

| EncryptMode | Windows 写入端 | Windows 读取端 | Linux当前构建 | 当前结论 |
|---:|---|---|---|---|
| 1 | `sub_10001190`: A7F0, 密钥=MD5(密码) | `sub_100384E0`: A6B0 逆变换 | `fileKey_Decrypt case1 -> Decrypt` | 算法双向闭合；22盘正向样本0 |
| 2 | `sub_100036E0` 或 `sub_10011010`: 标准SM4-ECB | `sub_10028AB0 case2`: 标准SM4 解密 | `MC_KKSMS4::DecryptBuffer` | 44条真实条目；已闭合 |
| 3 | `sub_1000FC10`: AES-128-ECB | `sub_1002F670`: AES-128 逆变换；CRC失败后强制模式1重试 | 当前 `fileKey_Decrypt` 无case3 | 算法/兼容行为闭合；22盘正向样本0 |

继续向上追写入端可达性后，已经能排除“模式1/模式3 只是死代码”的解释：

- `CUsbRegsiter` 构造函数在 `0x10038EB4` 把 `this+0x6EC` 默认置为 `2`；
- 唯一设置函数 `sub_1003B4E0(arg)` 直接覆盖 `this+0x6EC`；
- `WriteNormalULabel` 对请求字节 `arg+0x7E8` 做正式映射：值1调用设置函数(1)，值2调用设置函数(3)，其它值调用设置函数(2)；
- `CreatePartitions` 三处 16B 封装密钥写入端都读取同一个 `this+0x6EC`：2走SM4、1走A7F0、3走AES-128，并把其低字节原样写到每条紧凑布局条目的 `EncryptMode@+0x58`；
- `usbtoolBusManage.dll::UsbtoolBusMgrInter::LabelInfo::Print` 把 `LabelInfo+0x7E8` 明确打印为 `crypt=%d`，因此底层请求字段已经有上层业务名 `crypt`；
- 制标 UI 的 `tabAlgorithmComboBox` 实际加入 `SMS4`、`AES`、`AES_CROSS` 三项并默认索引=0；`currentIndex()` 在 `0x467270..0x467282` 直接写入策略对象 `normalDetail+0x44`，日志字段名为 `normalDetail.algorithm`。

此前保留的 UI 桥接缺口现已由同一 `cemssafeudisklabeltool.exe` 直接闭合：
`tabAlgorithmComboBox::currentIndex()` 写 `normalDetail+0x44`；`sub_42E8E0` 随后执行
`LabelInfo.crypt(+0x7E8) = normalDetail.algorithm(+0x44)`，反向 `sub_42EC90` 又执行
`normalDetail.algorithm(+0x44) = LabelInfo.crypt(+0x7E8)`。组合框初始化汇编 `0x47780F..0x47788C`
构造 `AES_CROSS/AES/SMS4` 后按栈顶实参顺序向 QStringList 实际追加
`SMS4 -> AES -> AES_CROSS`，并 `setCurrentIndex(0)`。因此数据流精确为：

- `SMS4(index0) -> crypt0 -> EncryptMode=2`；
- `AES(index1) -> crypt1 -> EncryptMode=1`；
- `AES_CROSS(index2) -> crypt2 -> EncryptMode=3`。

UI、normalDetail、LabelInfo 与 CreatePartitions 的三档模式分派现已成为连续可验证数据流。

模式1 的关键固定关系：

```text
key = MD5(effective_password)
A7F0 key material = key XOR "EDPSECDISK200709"
wrapped16 = A7F0(file_key16, key, counter=0)

reader:
file_key16 = A6B0(wrapped16, key, counter=0)
CRC32(file_key16) == FileKeyCRC
```

模式3 则是：

```text
key = MD5(effective_password)
wrapped16 = AES-128-ECB-ENC(file_key16, key)

reader:
file_key16 = AES-128-ECB-DEC(wrapped16, key)
CRC32(file_key16) == FileKeyCRC
```

Windows `UserLogin` 还明确实现模式3 历史回退：第一次模式3 解包
CRC失败后，以模式1 重解同一 16B 封装密钥；第二次 CRC 成功则接受。
对应日志分别为 `EncryptMode == eEncryptAESOPENSSL` 与
`dwKeyCrcOld == m_epiNewInfos[nIndex].FileKeyCRC`。

`oldSM4=="1"` 也不再视为独立盘面配置类型。该分支
`sub_10011010` 与默认 `sub_100036E0` 均逐常量/轮函数对应标准SM4：
同一 S-box、FK、CK、32轮和同一块输入输出语义；Windows 模式2
读取端只有一个标准SM4解包分支且完全不读取 `oldSM4` 配置。
所以它只是实现选择，不改变盘面格式。

因此 `+0x38..+0x47` 整体仍记 **部分闭环**，但剩余原因已收缩为：
**22份原始盘没有模式1/模式3 的正向样本**。当前44条加密条目全部模式2；
UI→加密→EncryptMode 的桥接和数值映射已经闭合。严格完成度统计仍不增加
16B×3，避免用静态算法闭合替代真实盘证据。

扩展历史语料又改成**完全不依赖设备 ID 文件名或旁挂文件**的只读统计集：
对 `/Users/zhangyuxi/Desktop/u_disk` 下3896个至少13扇区、至多1GiB的候选，先以固定
LBA6 滚动密钥解出 `m_crcUsbID[0]@+0x100`；该 DWORD 就是
`CRC32(device_id)`，可直接作为 LBA12 A6B0 密钥。只计解密后条目0=`EDPF`、
条目数量=1..3 且所有0x60 步长条目魔数有效者。最终得到58份有效捕获：
49份模式元组=`[0,2,2]`，9份=`[2,2]`，**模式1/模式3仍为0**。
该扩展集合含历史/转换状态，只作为配置类型搜索负证据，不并入22份严格原始样本，
也不提高完成度。

### 6.3 LBA12 +0x48..+0x5F：扩展密钥材料与保留[7] 必须分开

两个 Linux 组件使用不同 ABI：`libedpedisk.so` 运行时的
`tagNewEdpPartionInfo`/头大小明确为 `0x60=96B`；
`libcemsfilesyscheck.so` 的 DWARF 则是 `sizeof=0x68=104B`，
其中 `+0x50 EncryptFileKey32[16]`、`+0x60 EncryptMode`、
`+0x61 Reserved[7]`。同名 C 结构不能按偏移直接混用。

对 Windows 96B 紧凑布局运行时：

- `CreatePartitions / sub_1003DB50` 首先执行
  `memset(var_1364, 0, 0x120)`，一次清零完整3×96B 条目；
- 后续 16B 封装密钥只写 `+0x38..+0x47`，EncryptMode 写 `+0x58`；
- `UserLogin` 从每条条目 `+0x38` 只复制16B，并从 `+0x58` 读取模式；
- `sub_10026050` 改密码同样只读写16B `+0x38..+0x47`；
- Linux `libedpedisk.so::SetPartitionNewPass @ 0x52520` 对
  版本=0x0206 也只更新16B `+0x38`。

22份原始真实设备参考集的66条 EDPF 条目重新统计：
`+0x59..+0x5F Reserved[7]` 为 **66/66 全零**。
这7B同时具备正式字段名、写入端零来源、负向消费端和实盘闭环，
因此三个条目共 **21B 部分闭环 -> 完全闭环**。

相邻 `+0x48..+0x57` 后续又完成了单独的跨代闭环，不能再停留在
“存在字段名，所以用途未知”的阶段：

- 旧 `tagEdpPartionInfo` 是 **72B ABI**，只到 `EncryptFileKey@+0x40`，
  **旧版 72 字节 ABI 不存在 EncryptFileKey32 槽**；
- 104B 检查器 ABI 才正式增加 `EncryptFileKey32[16]@+0x50`；
  `GetPartionFromOld` 把旧72B 条目转成104B时不填该数组；
- `CDiskReader::DecryptFileKey` 只读自然对齐 `+0x40..+0x4F` 的16B主封装密钥
  和 `+0x60 EncryptMode`；`CheckFileKeyCrc`、`ReadFileSysSector0`、
  `DecryptFileSysSector0` 同样没有 `+0x50` 值相关读取；
- 紧凑布局主挂载库 `libedpedisk.so` 构造 `PartitionHeader` 时会按值缓存完整96B，
  所以紧凑布局 `+0x48..+0x57` 会结构性进入对象 `+0x88/+0x90`。但按
  `PartitionHeader` 符号边界逐函数审计，这两个对象QWORD除构造写入外没有任何
  算法读取；正对照紧凑布局封装密钥 `+0x38..+0x47` 映射到对象 `+0x78/+0x80`，
  其中 `+0x78` 被 SMS4/AES128/OldEdp 三套解密路径实际取址并按16B消费；
- Windows 当前 `CreatePartitions` 先把3×96B整表清零，后续不覆写紧凑布局
  `+0x48..+0x57`；Windows UserLogin/改密也只使用主 16B 封装密钥与 EncryptMode；
- 22份原始真实设备参考集的66条现存条目中，该16B **66/66全零**。

因此这不是保留[7] 的一部分，也不是当前第二把活动文件密钥；它是正式保留名
`EncryptFileKey32[16]` 的 **跨代兼容槽**。旧ABI无槽，
新ABI保留并可随结构搬运，但当前已知算法链不消费；当前紧凑布局写入端显式零。
这满足本项目对“结构缓存但无语义消费”区域的完全闭环标准，三个条目共
**48B 部分闭环 -> 完全闭环**。未来若其它独立ABI出现非零该槽，兼容实现应结构保留，
不能因为当前66/66为零而强制清零。

**LBA12 EncryptFileKey32 兼容槽结构缓存 / 负语义消费端闭环**
与 `Reserved[7]` 继续作为两个独立门禁，避免再次混成“23B 全零填充”。

## 7. 代码与测试门禁

### 7.1 CI 实盘子集

`tests/provision_protocol_audit.rs` 必须持续验证仓库中的真实原盘夹具：

- 只统计非免密夹具；
- LBA11 必须是 DRKB；
- random252 不得出现写入端不可能产生的 `0xFF`；
- 使用 ASCII VID/PID；
- DiskSize/CHS 至少有一种必须解出 PDKB；
- PDKB+4 必须与备份 device_id 一致；
- 数字小端 VID/PID 必须不能误解成功。

### 7.2 文档契约测试

`tests/protocol_documentation_contract.rs` 负责拦截文档口径回退：

1. LBA0–12 必须各自合计 512B；
2. 总字节必须 6656B；
3. 完全闭环总量不得低于当前基线；
4. 未知不得高于当前基线；
5. 每一条完全闭环字段必须填写写入端、消费端、实盘验证；
6. 主文档必须保留官方制盘工具链；
7. 主文档必须明确禁止把“样本全零/只有字段名/能生成”当完成。

## 8. 后续证据提升（不改变当前 6656B 完全闭环基线）

LBA0–LBA12 当前语义覆盖已经是 **6656/6656B 完全闭环**。后续工作只允许提升
物理配置类型覆盖、历史来源或协议边界之外的对象，不能把缺少某个旧可执行文件
重新解释成盘面字节语义缺口。

优先级：

1. **保持机器账本为硬门禁**：任何完全闭环都必须能从
   `audit/protocol/byte_ledger.tsv` 回指写入端 / 消费端 / 物理证据；
   虚拟与物理永不混写。
2. **补 `MISSING_PHYSICAL` 配置类型**：优先寻找真实 EDP GPT 启用、LBA12 模式1、
   模式3 和旧版 v0064 正例；在获得实盘前维持语义完全闭环 + 物理覆盖缺失，
   不得把官方虚拟输出冒充实盘。
3. **历史写入端来源**：继续追 join59、非零 `HSerialCRC[5]`、
   `UsbOnlyInfo=0`、旧版 MBR 快照等精确写入端/配置类型选择器，只用于解释
   “当年由哪个版本/调用者产生”，不回退已经闭合的盘面 semantics。
4. **LBA3 制造商 internals**：EDP 边界已经完全闭环为制造商负责
   不透明原样保留/忽略；若继续追 PS2307/PS2309、F2 标记页与固件映射，
   这是更深的厂商内部格式研究，见
   `audit/protocol/notes/lba3_manufacturer_boundary.md`。
5. **LCE 专项**：LCE 的 locator、负载、crypto、旧版消费端和 driver
   物理 I/O 已闭环；剩余的是上层业务“何时/为何主动重写 LCE”的 trigger 来源
   和历史版本等价性，见 `docs/protocol/LCE.md`。
6. **LBA0–12 之外的对象**：IIR 的真实物理绑定/封装链、设备尾部 forensic 窗口
   等必须单独建账，不能计入 LBA0–LBA12 的 6656B 状态。
7. **逐字节交付工具**：`scripts/protocol/query_byte_ledger.py` 继续作为原始字节 /
   配置类型 / 证据查询入口；只有注册正式解码器后才允许显示解密视图。

## 9. 操作安全边界

本审计阶段：

- 可以读取本地反编译文件、二进制、历史原始备份；
- 可以修改 edpcli 代码、测试、文档；
- 可以生成内存/文件中的模拟 LBA0–12；
- **禁止对真实物理原始 USB 执行写入**，除非用户再次明确授权。

## 10. 验证历程附录

> 本附录由原 `PROVISION_PROTOCOL_AUDIT_2026-09-19.md` 迁移而来。
> 保留每次逆向如何得到结论、哪些假设被反证、哪些代码/机器码/实盘相互印证。
> 其中出现的阶段性计数或已被后续证据推翻的结论，仅作为**验证历史**，不得覆盖前文
> `STRICT_PROGRESS` 与字段账本。

日期：2026-09-19
范围：旧审计集合共 23 份前部快照，但其中混入免密/实验态，不能再统称“23 份真实原盘”。当前生成协议参考集改为 22 份只读样本：`nopwd_tool/backup` 中 21 份非免密完整备份 + 1 份独立 SanDisk 原始加密盘；仓库保留 7 份裁剪后的原始 LBA0–12 协议夹具。全程只读，不对物理原始磁盘写入。

### 结论

1. onlyid 不是 device_id / VID / PID / 容量的确定函数；Windows 官方注册路径已经找到其生成源。
   - 同一 Netac 0dd8:2005、相同容量、相同 device_id 的真实样本存在多个不同 onlyid。
   - 样本同时存在大于 i32::MAX 的十进制文本和负数文本。
   - `cemsusbregsiter.dll` 的 `RegsiterUsb -> sub_1003d960` 明确执行 `CoCreateGuid()`，随后对 GUID 原始 16B 调用协议同款 `CRC32_bare`，结果写入对象字段 `+0x698`；`sub_10014550` 再把该 32 位值格式化进 LBA4 的 `$$$...$$$`。
   - 因此制盘可以自动生成 onlyid：`random GUID 16B -> CRC32_bare -> u32 bit pattern`。为克隆/重建已有标签身份，仍保留显式 onlyid 输入。

2. LBA12 0x170..0x200 的 144B 不是供体随机保留区。
   - 对全部已提交真实备份逐字验证：
     尾部 == a7f0_full(144B 全零, CRC32(device_id), initial_counter=0x170)。
   - 因此新盘可由目标 device_id 纯生成该区域，不复制供体。

3. 当前提交样本中的空白扇区策略已经有真实样本证据，但“当前样本全零”不等于协议上永远保留。
   - LBA1、2、5：当前 22 份生成协议参考样本全部为全零。
   - LBA3：严格 22份中21份为全零、唯一非零样本带 Kingston 制造标记
     `this is mp mark`；扩展历史备份又发现第二种标记配置类型，二者
     `+0x020..027` 不同，同型号也存在全零快照，因此不能把任一非零配置类型
     当成固定模板。
   - LBA10：21/22 全零；唯一非零样本就是独立 SanDisk 原始加密盘，前 0x80 经 A6B0 解密后为 `EESI`，后 0x180 物理全零。因此 LBA10 是可选设置扇区，不应继续命名为“保留扇区”。
   - 官方 `RegsiterUsb` 主路径中的 `0x0d` 已确认是扇区数量=13；从 LBA0 开始连续读写，协议范围因此严格为 **LBA0–LBA12**。当前备份 / 检查 / 制盘 / 恢复都统一使用这 13 个扇区。

4. LBA8 的 GLAB 标准值在所有可解码样本中一致：
   322CA28A-D7D1448B-DCE2CED9。
   - User / Dept 是动态业务字段。
   - Autonum 在历史样本存在不同世代，因此作为配置类型字段处理，不从供体复制。

5. LBA9 在历史原盘存在 EETU/SAPF 与全零两种合法形态；现有 apply 的最终免密形态会清零 LBA9。
   - 新盘制盘的第一阶段目标与现有免密产品形态一致，标准配置类型采用全零 LBA9。
   - 不尝试复制厂商/旧版本 SAPF 的未知附加字段。

6. LBA6、LBA7、LBA8、LBA11、LBA12 已有解码器可作为基础验证器，但本轮审计发现现有解码器有数处结构解释错误，必须修正后才能作为逐字节 oracle。
   - LBA6 校验和、设备 CRC；
   - LBA7 `rolling-XOR/EDPF`；
   - LBA8 `LLGB/User/Dept`；
   - LBA11 PDKB + 目标 device_id；
   - LBA12 A6B0/EDPF。

### 逐字节复核新增结论（2026-09-19）

以下结论不是直接采信 `u_disk` 文档，而是先用历史真实前部镜像重新独立复算，再把关键变体裁剪为当前仓库中的 LBA0–12 协议夹具；`u_disk` 只作为候选结论和反编译入口。

**证据口径更新（2026-09-19）：`nopwd_tool/backup` 中由 edpcli/旧工具执行免密转换后产生的快照只能用于产品回归，禁止作为“原始加密标签如何生成”的证据。其中 Aigo U335 `onlyid=2071754312 @ 12:09:32` 已由 MBR/LBA6/LBA7/LBA12 内容确认是转换后的免密状态，因此当前“原始生成协议”参考集仍使用其余 21 份非转换完整备份，再补入独立 SanDisk 原始加密盘，共 22 份。另一方面，`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4` 是 2026-08-23 从真实 SanDisk Ultra 免密码 U 盘只读采集的原始设备快照，不是 edpcli 自生成/转换盘；它作为独立的第 23 份**真实设备行为/配置类型证据**纳入 LBA7 等观测，但不替代 22 份“原始生成参考”去证明加密制盘写入端语义。已知局部实验态仍按 LBA 单独降权。仓库 `tests/provision_protocol_audit.rs` 对转换盘继续显式排除，并把该真实免密盘的 LBA7 单独放在 `protocol_evidence` 下，不让它进入原始生成参考循环。**

#### LBA3：EDP 原样保留现有，厂商 MP 语义仍未闭合

重新追当前 Windows `CUsbRegsiter::RegsiterUsb/sub_1003b560`：

- 入口先 `ReadSectorData(..., count=0x0D)` 读取现有 LBA0–12；
- SAFE6 注册分支明确调用 LBA4/LBA6/LBA8/LBA11 builders，并继续构造协议分区信息；
- 没有 LBA3 构造器，也没有对 `buffer + 3 * sector_size` 的负载解析/重建；
- 最终仍以同一暂存缓冲区执行
  `WriteSectorData(..., count=0x0D)`。

所以当前官方 EDP 写入端对 LBA3 的语义是 **原样保留现有字节**，不是
“强制生成 512B 全零”。Linux `libcemsfilesyscheck.so` 的独立符号/实现集同样只有
`BuildSector0/4/6/7/8/11/12`、GPT 0/1/2，以及
`ReadSector4/6/8/11/12`，不存在 `BuildSector3` / `ReadSector3`。
对 Windows 当前注册、登录、修复组件的字符串/调用路径复核也没有找到
LBA3 负载消费端。这只能证明 EDP 当前组件**不解释**该扇区，不能替代
厂商量产工具或控制器固件的消费端证据。

22份原始生成参考逐字节复核：

- 21/22 整扇全零；
- 唯一非零盘为 Kingston DataTraveler 3.0；
- 该盘的真实非零结构是：
  `+0x001=01`、
  `+0x020..027=b5 7e 9c 45 00 80 00 14`、
  `+0x1F0..1FF="this is mp mark\\0"`；
- 同 VID/PID 的另一 Kingston 原盘整扇全零。

随后对 `nopwd_tool/backup` 与 `utils/backup` 两个历史备份目录共60份 `.bin`
做只读扩展扫描；该扩展集合包含历史/转换状态，只用于配置类型统计集，不改变22份
严格原始代际参考的计数。非零 LBA3 只有3份，并精确归成两类：

- 2026-08-03 两份 Kingston 快照逐字节相同：`+0x001=01`、
  `+0x020..027=a8 82 a4 22 00 20 02 16`、
  `+0x1F0..1FF="this is mp mark\\0"`；
- 严格 2026-09-03 Kingston：`+0x001=01`、
  `+0x020..027=b5 7e 9c 45 00 80 00 14`、尾标记相同；
- 同型号其它快照还存在整扇全零配置类型。

本轮继续追制造端来源后，`"this is mp mark"` 已不再只是无法归属的 ASCII
尾标。USBDev.ru 的 Phison FW.BIN / 固件资料明确使用同一
`this is mp mark` 作为 Phison 固件/MP 标记；Falcon Sandbox 对真实
`MPALL_F1_9000_v372_0B.exe` 的静态结果又同时包含该标记、
`C:\\PhisonLog` 和多个 Phison 控制器型号串。因此 LBA3 的制造端家族
可以收敛为 **Phison MP/FW 制造元数据**，不再只写“未知厂商 MP”。

继续取得并离线逆向 FlashBoot 的 MPALL v3.72.0B 原包后，Phison 制造链又向前推进，
但同时否证了一个过强假设。`MPALL_F1_9000_v372_0B.exe` SHA-256=
`96614750c61e0ad6b05d19e74848c1679f6318dd21de6faee46c92fb05152142`，
与公开 Hybrid Analysis 样本完全一致。其机器码直接证明：

- `CBaseController::WriteF2Mark@0x00581B00` 把 `object+0x1C00C` 交给低层
  `fcn.004203D0`；后者构造 `06 06 01 ...` vendor command 并写 **0x200B**；
- 随后 `fcn.004202C0` 以 `06 05 ... "INFO"` 读取响应；
- `WriteF2Mark` 对读回缓冲区起始处与 `object+0x1C00C` 做完整 **0x200B `memcmp`**。

因此可以确认 MPALL 存在真实的 **512B F2 信息写入/读回路径**。但进一步
逐偏移审计证明，不能把这512B直接当成物理 LBA3：

- MPALL 的正式信息校验路径要求暂存开头为 `12 01 00 02`；真实 Kingston
  非零 LBA3 则从 `00 01 00 00` 开始，而且整扇均不存在 `12 01 00 02`；
- `GetInfo.exe` 在打印 `Get_Info_Page INFO` 后先清 `0x4D1EC0` 的0x210B，再把
  `0x4D1EC0` 与字符串 `INFO` 交给设备读取函数。相邻 `0x4D1CB0 / 0x4D1EC0 /
  0x4D20D0` 恰为连续三个0x210B 响应 work 缓冲区，对应版本/信息/RD 路径；
- GetInfo 的正式制造字段也位于完全不同的位置：`SampleMark` 由对象字段控制，
  写到原始信息响应 `+0xD9/+0xDA` 为 `12 56` 或 `00 00`；`MPF1F2` 则按
  控制器代际从信息 `+0x93` 或 `+0xDF` 的位域解析；这些偏移
  均与 LBA3 `+0x001/+0x020..027` 不对应；
- MPALL 包内两份 `FW*.BIN` 与两份 `BN*.BIN` 的 `"this is mp mark"` 都在各自
  **最后512B的 `+0x000`**，而实体 LBA3 标记在 **`+0x1F0`**；
- 两种真实 LBA3 的8B材料 `a882a42200200216` / `b57e9c4500800014` 以及各自
  4B半段，在已取得的 MPALL 可执行文件、FW/BN BIN 中均无直接常量命中。

所以本轮真正闭合的是：**Phison F2 标记/F2 信息制造生态确实存在独立512B
写入/回读协议，但“F2 信息暂存 == LBA3”已被结构反证。** `WriteF2Mark`
只能继续作为厂商家族/邻近制造路径证据，不能作为 LBA3 精确写入端记账。

控制器型号仍不能锁死。本地严格非零盘为 Kingston DataTraveler 3.0
`VID=0951/PID=1666`、121110528 个 512B 扇区，即 `62008590336B`；
Psychson 历史真实记录中同一 VID/PID/型号/物理容量至少同时出现：
**PS2307 + MPALL v3.34.07** 与 **PS2309 + MPALL v5.35.35**。
因此这些外部标识只支持 Phison 家族归属，不能证明本地盘必为某一个
PS22xx 控制器；后续应优先差分这两个精确量产代际的 F2 信息路径。

因此至少存在 **两个非零制造商/MP 配置类型 + 一个全零配置类型**；尾标记
可以作为 MP 负载家族锚点，但中间8B绝不是固定常量。新增
`tests/fixtures/protocol_evidence/kingston_20260803_mp_profile_lba3.hex`，其512B
原始扇区 SHA-256 为
`a1e1961d4ab452b6a2f277ee2027c962ea8bed58c6b85f05da12b247a706580e`，并以
`lba3_mp_marker_has_multiple_real_historical_payload_profiles` 对两个真实标记
配置类型做差异门禁。

因此 LBA3 不能继续作为“完全不知道边界”的未知，也不能把尾部 ASCII
误建模成一个独立 EDP 字段，更不能把某一份 `+0x020..027` 当成固定模板。整扇
512B 继续保持部分闭环：
**EDP 原样保留/忽略边界与 Phison 制造家族已闭合，MPALL/GetInfo 又提供了
独立 F2 信息路径及其与 LBA3 不同构的直接反证；但 LBA3 自身精确写入端、
`+0x001/+0x020..027/+0x1F0` 的生成规则/字段定义和控制器固件消费端
仍缺失，所以严格规则下仍是 0B 可计完全闭环。**

#### LBA4：必须区分历史原始全零空洞与当前 SAFE6 完整滚动

- `0x00..0x17`：明文 `$$$<onlyid>$$$` 及填充。
- `0x18..0x46`：有效滚动异或密文区，必须无条件按 16 位字解码；密文字节自然等于 0 也不能跳过。
- `0x47..0x1fb`：真实盘存在原始全零空洞与滚动加密全零两种物理表示；当前 22 份生成参考都没有在该437B恢复出非零业务负载。
- `0x1fc..0x1ff`：尾部滚动异或锚点，解密后为 `LLGB`。
- 新的“按区段处理”规则对当前 22/22 参考样本全部恢复：
  - `0x39..0x3c == "LLGB"`；
  - `0x1fc..0x1ff == "LLGB"`；
  - `u32@0x18 == onlyid ^ 0x88888888`。
- 旧 `inspect.rs` 的“原始单字节为 0 就恢复成 0”规则会把真实样本 `onlyid=949028302` 的 `LLGB` 错解为 `\0LGB`；本轮已改为只在整个 `0x47..0x1fb` 未写区全零时保留该区物理零，修复后当前 22/22 参考样本均恢复双 `LLGB` 锚点。

##### LBA4 `0x47..0x1FB`：437B 从“未知”到“部分闭环”再到“完全闭环”

本轮重新对齐 Windows PE 当前写入端、Linux DWARF 写入端/读取端与22份原始盘。Windows 当前
`CEMSUsbRegsiter.dll::sub_10014550` 与 Linux
`CLabelManage::BuildSector4@diskfile.cpp:741` 都只把0x2F 恢复节点写到
`+0x18..+0x46`；非空节点分支随后把滚动 XOR 继续覆盖到
`+0x1FF`，因此 `+0x47..+0x1FB` 只是随同保留底层字节一起被变换，并没有独立字段写入端。

Linux `ReadSector4@diskfile.cpp:957` 会对 `+0x18..+0x1FF` 执行同一滚动 XOR，
但最终只把 `decoded+0x18` 起0x2F复制给恢复节点输出并校验
`OnlyIdXor8`；不会向调用者返回或解释 `+0x47..+0x1FB`。

严格22份原始生成参考重新统计：

- 18/22（17份备份 + 独立 SanDisk）为物理原始全零空洞；
- 4/22 为几乎全非零滚动形态；
- 4/4 滚动形态按 onlyid 密钥解码后437B全零；
- 原始全零形态按区域规则保持后同样是语义全零。

因此这437B先具备了物理边界、当前完整写入端变换范围、读取端负向
语义消费端和真实双配置类型验证，从未知降为 **部分闭环**。随后继续追
一方运行时后，确认此前“必须找到原始全零最初写入端才能闭合”的前提本身
过强：这437B根本不是一个要求固定初始化值的业务字段，而是 **无所有者保留底层字节 / 表示载体**。

隔离 Unicorn 直接执行当前 Windows `BuildSector4/sub_10014550`，把
`+0x47..+0x1FB` 预填为任意非零 `0xA5`：

- 非空恢复节点分支：437B 原始字节全部进入滚动变换；独立按 onlyid 密钥
  反滚后 **437/437 精确恢复原始 `0xA5`**；函数本体没有在该区写任何业务负载；
- `arg0==NULL` 分支：同一437B **437/437 原样保持 `0xA5`**，函数完全不碰该区；
- 完整分支仍由函数自身重写尾部 `+0x1FC..+0x1FF=LLGB` 后再滚动；NULL 分支连尾锚点
  也不主动创建，进一步证明两条路径的区别是“变换现有表示”与“原样保留现有表示”。

同一完整滚动非零测试夹具再交给官方 Windows
`ReadSector4/sub_10015090` 动态执行；仅替换 MSVC `std::string/atoi` 运行库边界，滚动、
0x2F memcpy 与 `OnlyIdXor8` 校验均原生。读取端返回0，恢复
`onlyid=1625940067`、`OnllyID2Nd=main`、`LLGB`、版本=1；返回对象仍只有0x2F
恢复节点，437B 保留底层字节不进入 API 输出。

这与 LBA5 的不透明原样保留口径完全一致：**完全闭环描述的是字节的生命周期/所有权，
不是宣称当前样本中的零值是协议常量。** 原始全零历史最初是谁写入已经不再是业务语义
阻塞项；对未知非零保留底层字节，兼容实现必须原样保留或按完整分支可逆滚动，禁止清洗。

因此 `+0x47..+0x1FB` 共437B从部分闭环升 **完全闭环**。新增
`official_virtual_lba4_full_nonzero_backing.hex` / `official_virtual_lba4_null_nonzero_backing.hex`
与回归 `official_virtual_lba4_backing_is_unowned_and_representation_only` 固定任意非零正例。

强化门禁 `lba4_short_form_must_be_decoded_by_regions_not_by_zero_bytes`：
已提交真实测试夹具必须同时覆盖原始全零与滚动加密全零两种物理表示，
并断言两类样本的语义空洞都为全零[437]。

继续按身份节点形态与物理表示做交叉审计后发现一个关键反例：原始全零 **并不等于
旧版代际**。严格原始 Kingston `onlyid=1625940067 @ 2026-08-27 17:30:24`
同时满足当前身份条件 `OnllyID2Nd==main onlyid && HSerialCRC[5]==0`，但其
`+0x47..+0x1FB` 仍为原始全零；同设备 `17:28:57` 只读快照的 LBA0-13 与
`17:30:24` 逐字节完全一致。仓库新增 LBA4 单扇区证据
`tests/fixtures/protocol_evidence/kingston_20260827_current_identity_raw_zero_lba4.bin`，
SHA-256=`85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec`，以及
回归 `lba4_raw_zero_short_form_also_exists_in_a_current_identity_profile`。

因此短/完整表示的真实选择条件仍不能由第二ID/HSerial代际形态反推；
但在保留底层字节已闭合为无所有者表示载体后，这个选择条件只影响 **盘面
表示**，不再影响437B的业务语义完成度。它仍值得追踪用于历史复现，却不再是
这437B的完全闭环阻塞项。

##### 当前 SAFE6 分支纠偏：制盘现有短标准不是官方当前写入端

本轮回机器码确认 `sub_100880D0` 是标准 strcmp 语义：返回0表示字符串相等。
`CUsbRegsiter::RegsiterUsb` 对 `SAFE6` 相等分支会构造0x2F 恢复节点并调用
`sub_10014550(&restore_node, main_onlyid, LBA4)`；也就是说**官方当前 SAFE6
注册路径传非空恢复节点，执行完整滚动循环**。

审计开始时仓库 `src/provision/generate.rs::build_lba4` 仍在有效节点和尾部
LLGB 之间强制 `LBA4[0x47..0x1FB]=0`，并把这种原始全零短形式称作
“当前 Windows 写入端配置类型”。这与当前官方 SAFE6 写入端不一致，因而被
本轮列为阻塞项。下节完成服务器标志写入端/读取端/22盘闭环后，该阻塞项已
实际修复：制盘现使用完整滚动 + 异或后标志；历史原始全零实盘只保留
兼容读取，不能再作为新盘标准。

##### LBA4 `+0x45/+0x46`：异或后家族已扩展到 v19；检查不再按身份猜表示

Linux DWARF 将恢复节点最后2B正式命名为：

- `node+0x2D = bDataToServer`；
- `node+0x2E = bConnetServer`。

当前 Windows/Linux 与历史 Windows v19.11.4.1 已独立闭合相同的异或后
盘面规则：

- Windows `sub_10014550`：完整滚动循环后执行
  `LBA4+0x45=node+0x2D`、`LBA4+0x46=node+0x2E`；
- Linux `CLabelManage::BuildSector4@0x1D08E, diskfile.cpp:740`：
  `0x1D278..0x1D2F0` 做完整 0xF4-字滚动，随后
  `0x1D302..0x1D329` 同样异或后写回这2B；
- v19.11.4.1 `fcn.10006090`：滚动后于 `0x10006249..0x10006257` 把
  `node+0x2D/+0x2E` 写到物理 `+0x45/+0x46`。其 SAFE6
  `virtual_56@0x1000CC50` 可从对象/请求复制旧版 HSerial 材料到节点，
  因此异或后表示并不要求 `OnllyID2Nd==main && HSerial==0`。

读取端也已逐指令复核：

- Windows `sub_10015090` 统一滚动后复制 0x2F 节点，只校验
  `OnlyIdXor8`，不恢复两字节；
- Linux `ReadSector4@0x1E048, diskfile.cpp:956` 在
  `0x1E18C..0x1E1D8` 做同一滚动，再 memcpy 0x2F 节点，仍只校验
  `OnlyIdXor8`，同样没有补偿异或后存储。

所以这里存在真实的写入端/读取端非对称，但应限制在 **已证明的异或后写入端
家族**，不能限制在某种身份形态。对这类写入端，物理原始标志就是
写入端节点标志；通用滚动后得到的是官方读取端变换后字节。另一些
older 物理 samples 与普通滚动表示相容，但精确写入端仍缺。

旧22份原始生成参考的统计集仍保留为历史观测，不再作为表示分类器：

- 22/22 物理字节与通用滚动输出不相等；
- 6/22 当前身份：同时满足
  `OnllyID2Nd==main onlyid && HSerialCRC[5]==0`，物理=`00 00`，通用
  为6组不同非零值；这与当前 Windows `RegsiterUsb` 对节点整体清零、只赋值到
  `+0x2C`、再由 BuildSector4 异或后写回两个0字节完全吻合；
- 14/22 旧版身份：`OnllyID2Nd!=main onlyid && HSerialCRC[5]!=0`，物理非零，
  通用=`00 00`；
- 2/22 旧版身份配置（Aigo rev_pmap + SanDisk）：
  物理非零，通用=`0B 00`。

真实免密 SanDisk 进一步给出身份分类器的直接反例：第二=`0x4A32BA39`、
HSerial非零，盘面=`00 00`，当前官方读取端=`D4 D9`。仓库
`scripts/protocol/probe_lba4_reader.py` 固定 DLL SHA-256
`122b30301a7d23590f69313063414518f2b60d8535a57ee5d5a585a0c6b4c6eb` 与金标 SHA-256
`d6a935525b9e7bba9926a5ee1e2a74996d2aaf93bc6291723eaaedcff679a258`，隔离执行
`ReadSector4@RVA 0x15090`，实际覆盖到 `0x15295`、无异常，得到相同结果。onlyid 的
K0=`0xBFED`，两个标志位置的滚动密钥字节分别=`D4/D9`，因此盘面全零足以
机械地产生读取端 `D4 D9`，但不能反推写入端业务值。

实现已经同步修正：

- `src/inspect.rs`：先滚动解码并处理历史原始全零空洞，`decoded` 始终保持
  官方 ReadSector4 视图；不再根据第二/HSerial 覆盖标志。两个字段同时显示
  `reader=` 与 `wire=`，写入端侧明确标记为需要写入端来源；
- `src/provision/generate.rs`：当前 SAFE6 改为完整
  `+0x18..+0x1FF` 完整滚动，之后执行相同的2B 异或后覆盖；
- `src/provision/validate.rs`：不再接受“物理空洞全零”作为当前标准，
  而是精确重建官方完整滚动盘面镜像并比较；
- 历史原始全零短形式继续由检查兼容读取，不被删除。

上层消费端继续向下追后得到更严格的负边界：Windows
`ReadRestorInfo/sub_10041290` 自身不修正这2B；`ActiveNormalUDev -> sub_1003CEB0`
只消费恢复节点的身份材料；`GetUpLoadInformation/sub_10039C30` 会把节点
作为 API 输出的一部分带出去，但本 DLL 内不读取 `+0x2D/+0x2E`；Linux
`libcemsfilesyscheck.so` 除 BuildSector4 的两次异或后存储外，也没有其它
对这两个字段的直接访问。

继续把两个字节拆开后：

- `bDataToServer @ +0x45` 的旧版官方读取端逻辑视图确有非零 `0B` 配置类型。
  本轮按主文档调用方负责门槛补齐：v19 官方写入端原生注入 `node+0x2D=0B`
  后只改变盘面 `+0x45` 一个字节，证明写入端是透明序列化边界；严格 Aigo
  `onlyid=1987718388` 的盘面 `64 7A` 又按正式滚动精确还原为逻辑
  `0B 00`（密钥字节=`6F/7A`）。结合 Windows/Linux/2021 读取端的完整节点
  结构性保留以及恢复/上传路径无该字节值相关分支，本字节按
  **调用方负责兼容元数据** 升为完全闭环。更早普通滚动
  制造可执行文件仍未知，但只影响表示来源；
- `bConnetServer @ +0x46`：当前 Windows/Linux 与 v19.11.4.1 SAFE6 节点构造器
  都由全零初始化保持该字节=0；历史滚动形式加密原始样本的正式读取端
  视图同样为0。更关键的是 `scripts/protocol/probe_lba4_v19_writer.py` 保留真实免密
  SanDisk 的第二=`0x4A32BA39` 与非零 HSerial，只把节点标志设为`00 00`，在
  内存后端 I/O 中原生执行 v19 `fcn.10006090`，最终输出与真实 LBA4 **512/512
  完全一致**、SHA-256=`c26628566108031f439f999a46858476414ec8e7d1030a8293c9df0c463ad5d8`，
  且无模拟器异常。故真实读取端=`D9` 已正向闭合为异或后表示
  产物，而不是非零写入端标志。再结合 2021 历史修复的整9扇区原始复制
  行为与跨代负语义消费端，本字节重新闭合为 **完全闭环**。完全闭环不代表
  读取端必为0，也不把 v19 虚拟重建冒充该物理盘精确制造来源。

##### LBA4 `0x18..0x46` 官方结构与第二 ID

Linux DWARF 恢复出 `edpdiskglobal.h::tagEdpPartionRestorInfoNode`，总长 `0x2F`，正好对应 LBA4 `0x18..0x46`：

- `+0x00` / LBA4 `+0x18`：`OnlyIdXor8`，读写两端均闭合为 `onlyid ^ 0x88888888`；
- `+0x04` / LBA4 `+0x1C`：`OnllyID2Nd`；
- `+0x08..+0x1B` / LBA4 `+0x20..+0x33`：`HSerialCRC[5]`；
- `+0x1C` / LBA4 `+0x34`：`SingleUsbFlg`；
- `+0x1D..+0x20` / LBA4 `+0x35..+0x38`：`MyHardinfo`；
- `+0x21..+0x24` / LBA4 `+0x39..+0x3C`：`NewLabFlag`，当前样本为 `LLGB`；
- `+0x25..+0x28` / LBA4 `+0x3D..+0x40`：`Version`；
- `+0x29..+0x2C` / LBA4 `+0x41..+0x44`：四个扇区字段；
- `+0x2D/+0x2E` / LBA4 `+0x45/+0x46`：两个服务器标志。

Linux DWARF 同时恢复 `diskfile.h::UsbLabelParam`：`+0x278..+0x28B = HDOnlySerial[5]`，`+0x28C..+0x2AB = szOnlyID[32]`。该结构嵌在 Windows `CUsbRegsiter object+0x2E0`，所以此前把 `object+0x55C` 直接叫作 `onlyID2Nd` 是错误的；`object+0x55C` 实际只是 `HDOnlySerial[1]`。

当前 Windows 新建分支的真实写链是：`object+0x698 -> node.OnllyID2Nd`，而 `object+0x698` 已闭合为 `CoCreateGuid -> GUID raw 16B -> CRC32_bare -> main onlyid`。同时 `object+0x558..0x568 -> HSerialCRC[5]`。因此当前写入端配置类型的第二 ID 等于本次新生成的主 onlyid。

继续向运行时/服务器备份路径追后，`OnllyID2Nd` 的行为语义已不再只是字段名推断。
当前 `CEMSUsbRegsiter.dll` 形成了完整的 **备份加密 -> 服务器/存储 -> 激活解密**
闭环：

- `GetUpLoadInformation/sub_10039C30` 读取恢复节点后，先从盘面解出 LLGB 标签和
  v0x0202/v0x0206 EDPF 分区信息，拼成上传/备份数据块；最终调用
  `sub_10001190(..., key=restore_node+0x04, key_len=4)` 对整份数据块加密；
- `ActiveNormalUDev/sub_100399A0` 先 `ReadRestorInfo` 得到同一0x2F 节点，再进入
  `sub_1003CEB0`；后者直接把 `arg0+0x04`（即 `OnllyID2Nd`）作为4B 密钥种子
  传给 `sub_100012D0`，原地解密外部传入的激活数据块；
- 解密后的第一DWORD必须是 `LLGB`。成功后 `sub_10041480` 把 LLGB 标签内容写回
  LBA8；随后按尾部版本 `0x0206/0x0202` 拆出 0x120/0xC0 分区表，调用
  `sub_100414F0` 重建 LBA12；
- `sub_10001190` 与 `sub_100012D0` 的密钥 derivation 完全同构：对16个密钥字节，
  `key16[i] = key4[i mod 4] XOR base16[i]`，机器码中的 base16 精确为 ASCII
  **`EDPSECDISK200709`**。随后两者调用同一密钥序列，块变换分别走
  `sub_100028A0` 与 `sub_10002A00`，构成加密/解密对。

因此 `OnllyID2Nd` 应按行为命名为 **备份/激活加密密钥种子**；“第二ID”
只是历史结构名。当前写入端把主 onlyid 直接复用为该种子；旧版配置类型则保存
独立种子。对 `nopwd_tool/backup` 可由文件名提供主 onlyid 的22份历史捕获重算后，
15份旧版中 **15/15 第二密钥都不等于本设备组或全语料任何主 onlyid**；相同
主 onlyid 的重复捕获又保持第二密钥稳定，排除“上一次主 onlyid”解释。已提交
三份旧版测试夹具进一步锁定：NETAC_A=`44D9CE02`、NETAC_B=`028EFFD3`、
LEXAR=`7647B1EF`。

此前这里只缺旧版写入端；本轮已由官方历史二进制补齐。

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

也就是说，旧 SAFE6 写入端的第二密钥不是主 onlyid、设备 ID、MBR 签名
或宿主硬件号派生，而是**单独生成的一枚 GUID 的16个原始字节经协议同款
CRC32_bare 得到的32 位随机密钥**。当前写入端则取消第二次随机生成，直接把
主 onlyid（它本身也是 `CoCreateGuid -> CRC32_bare`）复用到 `OnllyID2Nd`。

这解释了此前所有真实配置类型：

- 当前：`OnllyID2Nd == main onlyid`；
- 旧版：`OnllyID2Nd` 与主 onlyid 独立；
- 同一已制盘标签的重复备份中第二密钥保持稳定，因为随机只发生在制标写入时，
  后续只是持久化读取；
- 已提交 NETAC_A/NETAC_B/LEXAR 的精确第二密钥继续锁定为
  `44D9CE02/028EFFD3/7647B1EF`；
- 回归进一步证明这些旧版密钥既不等于主/其它主 onlyid，也不等于
  `CRC32(device_id)`、LBA0 磁盘签名或 `MyHardinfo`，防止再次把随机
  备份密钥误归类为目标盘/宿主机身份。

结合已经闭合的备份加密 / 激活解密双向消费端，
`OnllyID2Nd` 的代际差异现在只剩“种子来源不同”，其业务语义没有分叉。
因此 LBA4 `+0x1C..+0x1F` **4B 从部分闭环升完全闭环**。

这一结论本轮又回到 **PE 机器码**重新核验，避免依赖 `Hex-Rays` 风格 `.m`
伪代码的局部漏语句：

- `RegsiterUsb@0x1003BBBB` 先把 0x2F 字节恢复节点清零；
- `0x1003BBD1` 读取 `object+0x698`；
- `0x1003BBD7` 写入 `ebp-0x3C`，按该节点的栈基址精确对应
  **`node+0x04 OnllyID2Nd`**；
- `0x1003BBF0..0x1003BC79` 依次读取
  `object+0x558/+55C/+560/+564/+568`，并写到
  **`node+0x08/+0x0C/+0x10/+0x14/+0x18`**，即五个
  `HSerialCRC DWORD`；
- 随后 `sub_10014550(node, object+0x698, LBA4)` 再写
  `node+0x00 = parsed_main_onlyid ^ 0x88888888`，复制完整 0x2F 节点，
  并执行滚动异或。

对应的 `.m` 反编译文本漏掉了 `node+0x04 = object+0x698` 这一条；
因此后续审计以这里的原始机器码为准。新增回归明确锁定
**LBA4 当前写入端机器码节点布局**，避免再次被反编译变量布局误导。

继续向上追当前 API 还得到一个重要边界：

- `WriteNormalULabel -> sub_10047690` 会把上层请求写入嵌入的
  `UsbLabelParam`；
- 它覆盖的最后几个相关块到 `UsbLabelParam+0x268` 为止；
- Linux DWARF 已知 `HDOnlySerial[5]` 在
  `UsbLabelParam+0x278..+0x28B`；
- 当前 `sub_10047690` **没有任何对 +0x278 这20B的写入**。

所以当前注册接口并不存在“把某个新算出的 5×DWORD HSerial 填进去”的步骤；
当前风格 HSerial 为零来自当前对象初始化/未提供输入这一配置类型。
这比“当前写入端恰好写零”更精确，但仍不能解释旧盘的非零五元组。

当前 22 份参考样本分三组：

- 6/22：`OnllyID2Nd == main onlyid` 且 `HSerialCRC[5] == 0`，完全吻合当前 Windows 写入端；
- 14/22：`HSerialCRC[5]` 固定为 `00001D29, 0000007B, 000004DD, 00000079, 0000007C`，跨 Aigo/Lexar/Netac 等厂商复用，但 `OnllyID2Nd` 随标签实例变化；
- 2/22：另一组高熵 `HSerialCRC[5]`，Aigo/SanDisk 之间部分成员重合。

本轮按严格“21份非转换备份 + 独立 SanDisk 原始加密盘”的 22份生成参考集
重新做交叉统计，得到一个此前没有单独写死的强约束：

- **6/22**：`OnllyID2Nd == main_onlyid`，同时 `HSerialCRC[5] == 0`；
- **16/22**：`OnllyID2Nd != main_onlyid`，同时 `HSerialCRC[5] != 0`；
- 两个条件在当前 22 份原始生成参考上是 **22/22 双向等价**，没有交叉反例。

这里特别排除了 `nopwd_tool/backup` 中未带 `_nopwd_` 名称、但内容已经确认
属于免密转换态的 Aigo `onlyid=2071754312 @ 20260828_120932`，并补回独立
SanDisk 原始加密盘；如果直接扫备份目录会得到错误的 7/15 计数。

这进一步说明 `OnllyID2Nd` 与 `HSerialCRC[5]` 至少属于同一代
恢复节点配置类型，而不是两个可独立任意组合的字段；但“相关”仍不等于
已经找到旧写入端。

本轮又把当前写入端向结构构造层前推：

- Linux DWARF 明确给出
  `UsbLabelParam.HDOnlySerial[5] @ +0x278..+0x28B`；
- Linux `UsbLabelParam::UsbLabelParam()` 在
  `diskfile.cpp:553..556` 对完整 `0x2AC` 结构执行清零；
- Linux `UsbWriteParam::UsbWriteParam(UsbLabelParam&)`
  在 `diskfile.cpp:558..568` 逐项复制单位、部门、姓名、GSerial、标签、
  AutoID、备注等字段，但**没有复制 +0x278 的 HDOnlySerial[5]**；
- Linux `CLabelManage::BuildSector4@diskfile.cpp:741..` 接收的是已经构造好的
  `tagEdpPartionRestorInfoNode*`，只负责把 0x2F 字节节点写入 LBA4、
  写尾部 LLGB 并执行滚动异或；函数内部**完全不生成 HSerialCRC**。

Windows 与之完全同构：

- `CUsbRegsiter` 内嵌 `UsbLabelParam` 基址是 `this+0x2E0`；
  因而 `HDOnlySerial@+0x278` 精确对应 `this+0x558..+0x568`；
- `sub_10047690(request, this+0x2E0)` 填充当前 `UsbLabelParam` 时
  同样跳过 HDOnlySerial 20B；
- `RegsiterUsb` 进入正式制标前还调用
  `sub_100139F0(this+0x2E0 -> temp UsbLabelParam)`，该拷贝函数也逐字段复制，
  **再次跳过 +0x278..+0x28B**；
- 最终 LBA4 写入端才从 `this+0x558..+0x568` 读取 5×DWORD 写入
  恢复节点。也就是说当前路径从默认构造、请求转换、临时拷贝到
  最终序列化，均没有任何“计算 HSerial”的步骤。

因此当前配置类型的 `HSerialCRC[5]=0` 已经不是单点观察，而是
**跨 Windows/Linux 两套官方写入端的零来源闭合**。但是旧16份非零配置类型
使用的“第二 ID + 5×DWORD HSerial”上游注入接口/算法仍未在现存官方构建中找到；
当前收集的 `usbtoolbusmanage.dll` 上层 `BusManageImp::WriteNormalULabel`
也未出现主机硬盘序列/DeviceNumber 到这20B的连接。

所以本轮仍不把 `HSerialCRC[5]` 升为完全闭环：完成的是当前全零写入端
和旧版/当前配置类型边界，而不是旧版非零写入端 + 消费端。
新增门禁 `lba4_current_writer_profile_never_carries_legacy_hserial_material`
在仓库提交的7份完整原盘子集中固定“当前镜像 -> 全零 /
旧版 非镜像 -> 非零”的关系；22份完整统计继续由本机原始证据集审计。

同一轮 22份原始盘还重新统计了恢复节点后半：

- `SingleUsbFlg @ LBA4+0x34`：22/22 = 0；
- `NewLabFlag @ +0x39..0x3C`：22/22 = `LLGB`；
- `Version @ +0x3D..0x40`：22/22 = 1；
- 四个扇区字节 `+0x41..0x44`：22/22 = `08 04 0C 01`；
- `MyHardinfo @ +0x35..0x38` 与两个服务器标志 `+0x45/+0x46`
  则明显随配置类型变化。

本轮继续把 `MyHardinfo` 与其它已经恢复的身份字段做逐盘交叉，得到一个新的
强约束：**严格 22份原始参考逐盘均满足
`LBA4.MyHardinfo == LBA8.HDSerialInfo`**。这不是“多数相同”，而是22/22精确相等：

- 当前配置类型：两处同时为0；
- 旧版配置类型的非零集合为
  `A017AD78 / A68BAE08 / 8B4613F5 / 2AB0E33C`，两扇区逐盘完全镜像；
- 同一 Netac `0dd8:2005` 的多个不同 onlyid 都稳定为 `A017AD78`；
- 该DWORD逐盘既不等于 `CRC32(device_id)`，也不等于 MBR 磁盘签名，排除
  “U盘自身ID/MBR值的简单副本”解释。

当前写入端也进一步闭合：`RegsiterUsb@0x1003BBBB` 对完整0x2F 恢复节点
执行 `memset(0)`；随后写 OnllyID2Nd、HSerialCRC、SingleUsbFlg、LLGB、版本、
扇区元组，却**没有任何对节点+0x1D..0x20 的覆盖**，因此当前
`MyHardinfo=0` 是正式写入端行为，不是样本巧合。

这把字段角色从不透明配置类型 DWORD 收敛为 **LBA8.HDSerialInfo 的镜像
主机身份信息 兼容复制**。但严格完全闭环门槛仍未跨过：2020
HDSerialInfo 写入端家族已知，而严格旧版盘对应的更早写入端 / 将同值写入
LBA4 MyHardinfo 的直接复制点仍未定位，所以本轮只增加语义证据，不增加完全闭环字节数。
新增回归 `lba4_myhardinfo_mirrors_lba8_hdserialinfo_in_original_profiles` 锁定零/非零双配置类型。

当前 Windows 机器码确实在节点清零后显式写入前四项固定值：
`SingleUsbFlg=0`、`NewLabFlag=LLGB`、`Version=1`、
`08 04 0C 01`。当前 Windows/Linux `ReadSector4` 路径都会把完整
0x2F 节点在滚动解码后结构性返回，只对 `OnlyIdXor8` 做强校验，
而对这四组固定字段没有值相关分支。按当前仓库已经统一用于只写 / 兼容
元数据的完全闭环标准，这种**结构性保留 + 语义忽略** 是明确的
消费端行为，而不是“消费端未知”。因此把这17B重新拆分：

- `SingleUsbFlg@+0x34` 1B：完全闭环；
- `MyHardinfo@+0x35..0x38` 4B：仍部分闭环，因值分配置类型且写入端/选择条件未闭合；
- `NewLabFlag@+0x39..0x3C` 4B：完全闭环；
- `Version@+0x3D..0x40` 4B：完全闭环；
- 扇区元组 `+0x41..0x44` 4B：完全闭环。

上述固定元数据共13B均有回归覆盖，但其中 `NewLabFlag` 4B 在旧36B LBA4基线中已经作为第二个 `LLGB` 锚点计入，因此本轮严格进度只净新增9B，禁止重复累计。新增回归把这些字段从代表盘扩展到全部已提交原始测试夹具；22份严格原始集
仍维持 Single=0 / LLGB / 版本=1 / `08 04 0C 01` 无反例。这里闭合的是
**固定恢复节点兼容元数据生命周期**，不是宣称这些值永远不能在
未来协议版本中变化。

特别是 6份 `HSerialCRC=0 && OnllyID2Nd=main_onlyid` 的当前风格
实盘，其服务器标志仍分别出现非零变化，说明“当前风格 HSerial”
不能进一步推导整个恢复节点都是当前 DLL 的同一静态配置类型。

这直接否定“`HSerialCRC[5]` 必然是当前 U 盘自身唯一序列”的强解释：

- 已提交 Lexar 与 Netac 真实夹具的 device_id、VID 均不同，但解出的
  `HSerialCRC[5]` 逐字节完全相同且非零；审计测试显式锁住这个反例；
- 22 份全量样本还呈现明显配置类型聚类：
  - 上述 14 份固定 HSerial 组全部同时表现为 LBA9 `EETU+SAPF`；
  - 当前写入端形态的 6 份 `HSerialCRC=0 && OnllyID2Nd=main onlyid`
    全部同时表现为 LBA9 `EETU+EPPE`；
  - 另外 2 份高熵 HSerial 样本同时是 LBA9 全零、LBA6 扩展区非零。
  这只能记为**格式/注册环境配置类型的相关性**，不能反推因果关系。

本轮又把 `bDataToServer@+0x45` 的非零读取端配置类型与这组代际指纹精确对齐：
旧备份集中唯一 `reader=0B 00` 的样本就是 Aigo U335
`rev_pmap / onlyid=1987718388`。独立按正式滚动算法解码得到
`HSerialCRC[5]=B5FF9C55/B39DAB28/7E8F5D4A/9AC9605A/7AC6D637`、
`MyHardinfo=8B4613F5`，物理标志=`64 7A`；同一份盘又是 LBA9 整扇全零、
LBA6 扩展区非零，并且是严格参考中的唯一 CHS MBR 配置类型。也就是说已观察到的
`bDataToServer reader=0B` 不属于 14份固定 `1D29/7B/4DD/79/7C + SAPF` 主流旧版
代，而与**高熵 HSerial / 全零 LBA9 / CHS** 的更老配置类型同现。这进一步缩窄了
缺失写入端的代际范围，但在取得那个写入端前，仍禁止把 `0B` 自行解释成某个
业务枚举值。

同一物理 Aigo U335 / 同一 `onlyid=1987718388` 还有一份 2026-09-16 免密转换后快照。
与 2026-08-27 原始加密备份逐扇区比较时，LBA0/LBA6/LBA7/LBA11/LBA12 均发生变化，
但 **LBA4 512/512 字节完全一致**（两份 LBA4 SHA-256 均为
`aad70723b3c1...`）。因此 `reader=0B 00`、高熵 HSerial 和 `MyHardinfo=8B4613F5`
都明确早于免密转换并被转换路径原样保留；不能再把这个标志配置类型解释为 nopwd 工具的
派生结果。回归 `lba4_old_server_flag_profile_survives_nopwd_conversion_bit_exact` 固定该纵向证据。

Linux DWARF 还把该节点的静态使用面收窄：`LPEDP_PARTION_RESTORINFO_NODE`
在 `libcemsfilesyscheck.so` 这个编译单元中只作为 `BuildSector4@diskfile.cpp:740` 与
`ReadSector4@diskfile.cpp:956` 的参数出现；`ReadSector4` 滚动后只整体复制 0x2F 节点，
随后唯一字段级判断是 `OnlyIdXor8`。所以该组件没有 `bDataToServer/bConnetServer` 的
值相关业务消费者。这个负向消费端证据缩小了 `+0x045` 的缺口，但旧 `0B`
写入端与其它上层组件是否消费它仍未取得，故状态继续部分闭环。


本轮又对已提交 Lexar + Netac A/B/C 四份固定 HSerial 原盘做跨字段负相关审计，
把两个容易误连的候选关系明确排除：

- 四盘的 `HSerialCRC[5]` 20B **逐字节完全相同**，均为
  `1D29 / 7B / 4DD / 79 / 7C`；
- 但 Lexar 的 `MyHardinfo/LBA8.HDSerialInfo=2AB0E33C`，Netac 三盘则稳定为
  `A017AD78`，因此这20B不是主机身份信息 DWORD 的展开/分片；
- 同一批固定 HSerial 盘的 SAPF 解码后尾部 `+0x114..+0x11F` 同时覆盖
  **全零、稳定非零和同一 Netac 后续变化**三种保留底层字节形态；特别是 Netac A/B
  尾部相同，而 Netac C 尾部已变化，但 HSerial 仍完全不变。

新增回归
`lba4_fixed_hserial_is_independent_from_hardinfo_and_sapf_backing`
把这组四盘反例锁死。因此旧 `HSerialCRC[5]` 不能再解释为
`MyHardinfo/HDSerialInfo` 的展开，也不能解释为 SAPF 32B 保留底层字节/尾部的缓存；
其旧写入端输入仍应继续沿独立的主机/注册环境身份链追踪。

主机身份候选链也进一步做了排错：

- Linux `libbusManage.so::UserInfo::GetHDiskSerialZ()` 会读取注册主机硬盘序列，
  因而“主机身份参与旧配置类型”仍是合理候选；
- Windows `vrvaud_c` 的确存在 `EDPUToolClientInfo/HDSerialCRC`，其来源已闭合到
  `DeviceNumber.dll::EDP_DiskNumber()`，失败时回退 `EDP_DeviceNumber()`；
- `EDP_DiskNumber()` 会枚举 `PhysicalDrive0..3`，并内含标准
  CRC32 多项式 `0xEDB88320`，但公开结果最终只是**单个 32 位 DWORD**；
- 当前反编译语料中 `DeviceNumber.dll` 只在 `vrvaud_c` 身份/策略链出现，
  未在 `cemsusbregsiter/usbtoolbusmanage` 注册写链发现连接；
- `cemsudisk` 的 `HDSerialNumber` 位于 `CallBackLog::BuildLog` 审计字段采集，
  同样不是标签写链；
- 对候选二进制做 20B 精确扫描，也没有发现
  `1D29/7B/4DD/79/7C` 或高熵 HSerial 数组的静态常量。

本轮又把 2020 `busManage.dll` 中 `"ReadUsbHserialsInfo failed"` 的调用点按
**接口 ABI** 追到底，得到比“没有发现 DeviceNumber 到 20B 的连接”更强的直接负证据：

- 字符串 `0x1002D05C` 的唯一代码交叉引用落在
  `fcn.10010730@0x10010730` 的 `0x100108CC`。实际调用位于
  `0x10010891..0x100108C6`：调用方先准备三个输出区，第一项由
  `DWORD 0 + memset(后续 0x2B)` 组成连续 **0x2F 字节恢复节点**，第二、第三项
  各为独立 4B DWORD，然后通过 CEMSUsbRegsiter 接口 `vtable+0x2C` 调用；返回0才打印
  `ReadUsbHserialsInfo failed`。这不是导入，也不是本地辅助函数，而是历史接口的虚方法槽。
- v19.11.4.1 `CEMSUsbRegsiter.dll` 的 `ISUdiskRegsiterObj` 虚表已精确定位到
  `0x1019DB54`；因此 `+0x2C` 正好是
  `ISUdiskRegsiterObj::virtual_44@0x100054A0`。该方法把**第一个输出指针**依次交给
  `fcn.100072C0 / fcn.1000DDA0 / fcn.1000DB90` 三个主/备读取端。主读取端
  `fcn.100072C0` 寻址到 `4*BytesPerSector`，完成 `$$$`/onlyid 校验与滚动解码后，
  以 `16 + 16 + 8 + 4 + 2 + 1` 字节的连续写入精确回填 0x2F 字节节点；其中
  `node+0x08..+0x1B` 的 20B HSerial 是**从盘面恢复节点解码回来**，不是此函数运行时生成。
- 只有恢复节点读取端成功后，`virtual_44@0x100055A2..0x100055BD` 才分别调用
  `UsbTools.dll` ordinal3=`EDP_DeviceNumber` 与 ordinal4=`EDP_DiskNumber`，并把两个
  返回值分别写入**第二、第三个 4B 输出指针**。因此旧 ABI 明确是
  `restore-node[0x2F] + DeviceNumber DWORD + DiskNumber DWORD` 三条独立输出通道，
  而不是“DeviceNumber 直接填 HSerial[5]”。
- 2020 `busManage` 成功后没有消费那两个主机身份 DWORD；它只把第一项
  0x2F 字节节点传给同一接口的 `vtable+0x44`。在 v19.11.4.1 虚表中该槽精确映射
  `ISUdiskRegsiterObj::virtual_68@0x1000E650`；恢复路径从传入节点只取
  `node+0x04 OnllyID2Nd` 作为恢复密钥，不读取 `node+0x08..+0x1B` 的 HSerial 值。
- 当前 `usbtoolbusmanage.dll` 又提供代际对照：`ActiveNormalUDev` 辅助函数
  `fcn.100AA7E0` 已改为通过新接口 `vtable+0x14` 直接调用 `RestoreRegsiterUsb`，二进制中
  不再存在旧 `ReadUsbHserialsInfo` 三输出调用形态，确认上述 `+0x2C/+0x44` 是历史 ABI，
  不是对当前接口的误配。

同一套 v19.11.4.1 写入端还把 **HSerial 的历史写入传输** 闭合到了 LBA4：

- `ISUdiskRegsiterObj::virtual_8@0x1000B9C0` 将
  `request+0x150/+0x154/+0x158/+0x15C/+0x160` 五个 DWORD 原样写到
  `object+0x2488/+0x248C/+0x2490/+0x2494/+0x2498`。
- SAFE6 `virtual_56` 构造 0x2F 恢复节点时逐项读取上述对象槽并放入
  `node+0x08..+0x1B`，随后调用 `fcn.10006090`。后者以 `4*sectorSize`
  寻址到 **LBA4**，把传入节点编码进 `$$$...$$$` 扇区并写回。因此历史传输链为
  `request+0x150..+0x160 -> object+0x2488..+0x2498 -> node+0x08..+0x1B -> LBA4`。
- 2020 `BusManageImp::ActiveNormalUDev` 对这份 00x184 字节请求先整体
  `memset(0)`，只填 `request+0xD0` 起的 00x40 字节字符串区；它不覆盖
  `+0x150..+0x160`，所以这条已取得调用方路径**确定**向五个 HSerial DWORD 输入全零。
- 继续穷举整个 2020 `busManage.dll` 后，这个负结论已从单一路径扩展到**全部已恢复
  请求构造器**：模块中只有 `fcn.10010020`、`fcn.100103B0`、
  `fcn.10010580`、`fcn.10010730` 四处先对 00x184 字节请求完整 `memset(0)`；四个
  函数逐指令扫描均没有访问 `request+0x150/+0x154/+0x158/+0x15C/+0x160`。由于
  请求基址固定为 `EBP-0x190`，五个 HSerial 槽精确落在
  `EBP-0x40/-0x3C/-0x38/-0x34/-0x30`，可以排除把其它栈局部变量误认成字段访问。
  因而**这个 2020 BusManage 模块中不存在另一条隐藏构造路径会填入严格旧版的
  非零 HSerial 五元组**；缺失写入端必须是更早或不同的调用方。
- `fcn.10008800` 从 `4*sectorSize` 连续读 `9*sectorSize`，即 LBA4-LBA12，
  再原样写到 **`disk_end-0x80000`**。该地址与 `fcn.1000DB90` 的第三读取端
  完全一致，所以这一候选是 LBA4-LBA12 的 512 KiB 盘尾镜像头，不是另一套 HSerial
  生成算法。

以上闭合的是“调用方输入槽 -> 对象 -> 恢复节点 -> LBA4/盘尾镜像”的持久化链；
严格旧版的非零五 DWORD 在进入请求之前如何生成仍未闭合。

因此当前不能把 `DeviceNumber/HDSerialCRC` 单 DWORD 与 LBA4
`HSerialCRC[5]` 直接等同；**旧 `ReadUsbHserialsInfo` ABI 已直接排除这种等价关系**。
这里仍不能进一步断言“更早写入端从未使用 DeviceNumber/硬件身份材料”：未知的更早调用方
仍可能存在某种 `host material -> 5×DWORD` 转换。由于严格旧版非零
`request+0x150..+0x160` 的真正赋值点/算法仍未找到，LBA4 这20B继续保持部分闭环，
不得据此增加完全闭环字节数。

##### 制盘 LBA4 标准修正（历史阶段；已被本轮 SAFE6 完整滚动证据部分推翻）

本轮发现制盘生成器曾把不同配置类型混在一起：

- `+0x18..+0x1F` 被错误当作 8B 随机 `lba4_nonce`；
- `+0x20..+0x33` 被硬编码成 14/22 样本中的旧配置类型
  `1D29,7B,4DD,79,7C`；
- `sparse_rolling_encrypt` 还把“明文为 0”错误解释成“物理字节保持 0”，
  与已验证的有效节点区连续滚动异或冲突。

这一阶段曾把新盘标准调整为：

- `OnlyIdXor8 = main_onlyid ^ 0x88888888`；
- `OnllyID2Nd = main_onlyid`；
- `HSerialCRC[5] = 0`；
- `0x18..0x46` 整个有效节点连续滚动异或，即使明文字节为 0 也加密；
- 短形式 `0x47..0x1FB` 作为整段“未写区”保持物理零；
- `0x1FC..0x1FF` 使用同一条继续推进的滚动密钥序列写入 LLGB 锚点。

`ProvisionEntropy.lba4_nonce` 与 `sparse_rolling_encrypt` 已删除；这一历史阶段的
校验器还曾逐字节检查完整47B 节点和短形式物理零区。

**本轮最新证据已经证明上面的“短形式 = 当前 Windows 写入端”结论不成立。**
当前 SAFE6 分支会传非空恢复节点并执行完整滚动循环。
紧接着的 `+0x45/+0x46` 审计已把 Windows/Linux 写入端、ReadSector4 非对称行为
和22份原始盘全部重新闭合，随后代码已经切换为真正的当前 SAFE6 完整
表示；历史原始全零形式只作为兼容读取配置类型保留。本段保留作为
“错误短标准是如何被证据推翻”的历史记录，不再代表当前实现状态。

#### LBA6：`0x1C0..0x1EF` 必须拆开

Windows `sub_10013fd0` 与 Linux `CLabelManage::BuildSector6(UsbWriteParam&, char*)` 两套独立写入端对齐：

- `0x1C0..0x1CF <- m_usbGSerial[0..14] + NUL`；
- `0x1D0..0x1DF <- BeiZhu[0..14] + NUL`；
- `0x1F0..0x1F3 <- m_encrypt`（低字节布尔值扩成 DWORD）；
- `0x1E0..0x1EF` 当前写入端没有显式覆盖；本轮已证明两份旧配置类型
  的非零内容不是独立扩展字段，而是**旧 MBR 分区表底层内容的残片**。

进一步核对 Windows 静态 `UsbMainBSec` 模板后，旧的
`0x1CA=128480` 解释可以撤销：

- 模板 `+0x1BE..+0x1CD` 本身就是标准 16B MBR 分区条目：
  `status=0, type=0x07, start_lba=63, sector_count=128480`；
- 模板 `+0x1FE..+0x1FF = 55 AA`，并含标准 MBR 启动代码 / 错误文本；
- 因而模板 `+0x1CA` 恰好只是这个 MBR 条目的 `sector_count` 字段位置；
- 但 BuildSector6 随后会把 `0x1C0..0x1CF` 整个 16B 区覆盖成
  `m_usbGSerial[0..14] + NUL`。所以最终盘面上的 `+0x1CA`
  已不再是模板 MBR 字段，而只是 **GSerial 固定槽内第 10..13 字节**。

22 份原始参考样本进一步直接否定“`+0x1CA` 是独立状态 DWORD”：

- 14/22：`u32@+0x1CA = 128480`；
- 2/22：`u32@+0x1CA = 20417`；
- 6/22：`u32@+0x1CA = 0x34314437`，小端字节就是字符串
  `"7D14"`，来自 `"322CA28A-D7D144"` 的中间四个 ASCII 字节。

因此同一偏移同时出现“模板几何值 / 另一几何值 / ASCII 文本”不是协议多态，
而是**错误地把字符串槽中的四个字节当整数解释**。

当前读取端也支持这个结论：

- Linux `ReadSector6` 把 `sector+0x1C0` 当 C 字符串，用于 GSerial 前缀匹配；
- `sector+0x1D0` 直接按字符串读回 `UsbLabelParam.BeiZhu`；
- 没有把 `+0x1CA` 作为整数读取，也没有发现 `0x1E0..0x1EF`
  的当前业务消费者。

Windows 新注册路径还解释了为何 NUL 后会出现看似“有规律”的尾字节：
`RegsiterUsb` 的本地写参数对象没有先整体清零，`sub_100139f0`
使用 strcpy_s 风格函数只复制到 NUL，而 BuildSector6 随后固定复制 15B
GSerial / 15B BeiZhu。当前 `BuildSector6` 自身则会先把临时16B缓冲清零，
再从输入槽固定取前15B并覆盖16B，因此当前配置类型的 NUL 后物理值取决于
上游输入槽的保留底层字节字节，而不是目标扇区底层内容。

旧配置类型的**值来源**必须单独看：两份旧版实盘的 NUL 后字节不是随机残值，
而是与后续 `+0x1E0` 连成一份结构完整的旧 MBR 分区表。但继续用一方
Windows `BuildSector6` 做非零保留底层字节正向实验后，槽本身的语义已经可以统一：在短
`GSerial="322CA28A"` 的来源 NUL 后人为预填 `A5×6`，以及空 BeiZhu 来源 NUL 后预填
`5A×14`，官方构造器都把这些字节原样复制到 LBA6；两个槽的 byte15 则仍由
零化临时固定为NUL。Windows/Linux 读取端又都只消费首个NUL前的C 字符串。因此
`+0x1C9..+0x1CE` 与 `+0x1D1..+0x1DE` 的**协议角色**是动态的“字符串正文或调用方负责
NUL 后保留底层字节”，而不是第二业务字段。旧版 MBR几何只是该保留底层字节的一种历史来源；
它仍值得保留取证，但不再阻塞这20B的字段语义闭合。新增
`official_virtual_slot_backing_lba6.hex` 与回归
`official_virtual_lba6_fixed_string_slots_preserve_nonsemantic_source_backing` 锁定任意非零
保留底层字节也必须原样保留。

全量原始样本当前分布：

- GSerial C 字符串：
  - 16/22 为 `"322CA28A"`；
  - 6/22 为 `"322CA28A-D7D144"`；
  - 16份短字符串样本 **16/16 在首个 NUL 后仍有非零字节**；
  - 在可解析 LLGB 的样本中都与 LBA8 GLab 前缀一致；
- BeiZhu C 字符串：
  - 20/22 为空；
  - 2/22 为 GBK `"普通"`；
  - 全部22份中有 **8/22 在首个 NUL 后仍有非零保留底层字节字节**；
- `0x1E0..0x1EF`：20/22 为模板零；2/22（Aigo U335 旧形态 +
  SanDisk 原始盘）保留旧版 MBR 分区表片段；
- `u32@0x1F0`：22/22 均为 1；官方写入端字段名是 `m_encrypt`，
  不能再标成“注册标志”。本轮进一步回到 Windows PE 原始机器码闭合其
  当前写入端：`RegsiterUsb` 的临时 `UsbWriteParam` 基址为 `ebp-0x3F4`，
  因而 `m_encrypt@+0x258` 精确映射到 `ebp-0x19C`；5字节比较目标
  `0x100C9A90` 为 ASCII `!SAFE`，相等分支 `0x1003BA94` 写1、非相等分支
  `0x1003BAC7` 写0，随后 `0x1003BAF5` 立即调用 `BuildSector6` 写到
  `LBA6+0x1F0..0x1F3`。因此“为什么当前样本为1”的写入端来源已闭合。
  继续核对消费端后，Linux DWARF 正式 `UsbLabelParam` 结构根本没有
  `m_encrypt` 成员；Linux `ReadSector6` 不返回该字段。Windows
  `CheckLabel/sub_100152A0` 在整段滚动异或解码和前508B 校验和验证后，
  显式读取 Dept/User/GSerial/标签等字段，却没有 `+0x1F0` 的值相关访问；
  已扫运行时组件同样未见行为消费端。独立 SanDisk 原始 LBA6 也保持值1。
  因此这4B由部分闭环升为 **完全闭环**，保守语义为
  **只写 `!SAFE` 标签代际元数据**：写入端保存当次匹配结果，
  校验和覆盖其物理字节，但读取侧不把它作为运行时加密开关。

本轮还重新从 Linux 官方二进制本身核对当前模板，而不是沿用旧文档：
`nm -S -C` 定位 `UsbMainBSec@0x22BB40,size=0x1000`，`.data`
原始字节显示模板相对 `+0x1E0..0x1EF`（VMA `0x22BD20..0x22BD2F`）
确为 16B 零；`BuildSector6` 从该模板起步且不再覆盖这一段。
这只闭合了 **当前配置类型的零来源**，不能解释两份旧配置类型。

进一步取得的历史 `CEMSUsbRegsiter.dll` v19.11.4.1（MD5
`783d01f19e998a514834bc5e5f4249ad`）把时间边界又向前推进了一代。SAFE6
上层 `fcn.1000B110` 把 Dept/User/标签/GSerial 等注册参数汇总后调用
`fcn.10006370`；后者对目标扇区执行 `rep movsd, ECX=0x80`，从静态
`0x101BA790` 精确复制 512B 模板，再写入各业务槽，随后对前 `0x1FC`
字节计算同款校验和，并以 `6 * sector_size` 定位后 `WriteFile` 一扇区。
直接读取该历史模板可见 `+0x1E0..+0x1ED` 为14B零，而 `fcn.10006370`
的全部字段覆盖项也没有触及这一区间。因此 **2019-11 的正式写入端
仍生成全零配置类型**，不能产生 Aigo/SanDisk 两份非零 MBR 快照。
这把精确写入端的搜索范围继续收缩到更早的 CEMS2.0/旧版写入端；
在取得那个同代写入端前，这14B仍不得升级完全闭环。

join59 一侧也进一步把“兼容读取端”与“同代写入端”拆开。CEMS2.0
`cems/Edp/fileophook.dll`（PDB 路径包含
`vrvrsms2.0\\Cems2.0\\trunk`）的 `fcn.10022f80@0x10022F80`
在 `0x100232ED` 比较 `0x40245E2A` 标记；命中后先复制
`0x3C=60B` 内联前缀，再把 LBA9 `+0x80` 续段写到输出
前缀基址 `+0x3B`，所以 Dept[59] 会被续段首字节覆盖。其 x64
同源构建也保持同一 `+0x3B` 接缝。这是可直接复核的
**CEMS2.0 join59 读取端仅**，并不能证明写入端也以59B切分。

为避免把“旧读取端 ABI”反推成写入端，本轮又对本机 VRV 树中所有包含
`0x40245E2A` 常量的二进制做了指纹筛选。实际 `mov marker` 的写入端
只落在当前 `cemsusbregsiter.dll` 与 `vrvaud_c.dll`，两者都明确复制
`0x3C=60B`；其余命中均为 `cmp marker` 的读取端。换言之，
**本地现有全部标记写入端都使用 60**。结合 v19.11.4.1
`fcn.10006370` 本身根本没有长 Dept 标记，现有证据只能证明
“CEMS2.0 读取端固定 join59 + 当前写入端固定 join60”，还不能闭合
产生 Lexar join59 实盘的精确代际写入端/配置类型选择。因此
LBA6 `+0x03F` 与 LBA9 `+0x080..0x0FF` 继续保持部分闭环。

本轮又把当前 `cemsudisk.dll::sub_101015e0` 的兼容选择条件收敛到单个
盘面字节，而不是继续把它笼统记成“代际模式”。固定 SHA-256
`32e88065725ccb9bc50e24c244f5686bf1d38335f58737f454a8f4ca2c892fd1`
后，机器码 `0x10101910..0x1010191B` 先把标记后 **15 DWORD = 60B**
内联 Dept 前缀复制到输出；紧接着 `0x1010191D` 读取源局部
`[ebp-0x6D]`。该地址正好是从 `[ebp-0xA8]` 开始的60B中第60个字节，
即 **serialized Dept[59] 本身**。随后：

- Dept[59] == 0：`0x10101934` 把128B LBA9 续段写到输出
  `+0x7B`，因此续段覆盖 Dept[59]，形成 join59；
- Dept[59] != 0：`0x1010194F` 改写到 `+0x7C`，从 Dept[60] 继续，
  形成 join60。

同一审计同时锁定当前 `CEMSUsbRegsiter::BuildSector6@0x10013FD0` 的
反向边界：`0x1001402D` 只有在 `strlen(Dept)>=64` 时进入长标记
分支，`0x10014077` 固定复制60B 内联前缀，`0x100140C4` 又固定从
`Dept+0x7C = Dept[60]` 取续段写到 LBA9+0x80。因此当前写入端
不可能自行生成“长 Dept 但 Dept[59]=NUL”的 join59 盘面。
`scripts/protocol/audit_join59_selector.py` 固定两份 DLL SHA 并重放这些
指令约束。这个结果闭合了 **消费端/配置类型选择规则**，但没有提供
缺失的历史写入端，所以严格字节状态不变。

两份旧配置类型的实际解密字节是：

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

因此 `+0x1E0..0x1ED` 正好是**第3条 MBR 条目丢掉前2B后的连续14B**：

- `+0x1E0..1E1 = start CHS 的后2B = C1 FF`；
- `+0x1E2 = partition type = 0x07`；
- `+0x1E3..1E5 = end CHS = EF FF FF`；
- `+0x1E6..1E9 = start_lba`；
- `+0x1EA..1ED = sector_count`；
- `+0x1EE..1EF` 已进入第4条 MBR 条目，两个旧样本均为 `00 00`。

CHS 也不是“看起来像 MBR”的弱匹配。两块独立非零实盘的完整条目3
前8B 都是 `00 00 C1 FF 07 EF FF FF`；按传统 MBR CHS 编码解码：

- 起点 CHS = `C=1023, H=0, S=1`；
- 终点 CHS = `C=1023, H=239, S=63`。

即两端都处于传统 CHS 柱面上限的饱和值，并呈现 240-头部/63-扇区风格。
配合下述 `start_lba/sector_count` 与 LBA12 type4 的 2/2 精确相等，说明
`+0x1E0..+0x1ED` 的 14B 已全部可以解释为 **旧 MBR 条目3 的结构字段**，
而不是未知私有字段。严格状态仍保持部分闭环的唯一原因是尚未取得把这份
旧 MBR 快照带入 SAFE6 保留底层字节的精确写入端/配置类型选择。

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

两份都是 **类型/起点/数量全匹配**。而严格22份参考进一步给出关键反例：

- 2/22 旧版片段非零，2/2 都与 LBA12 type4 几何精确一致；
- 20/22 `+0x1E0..1EF` 全零；
- 这20份的 LBA12 **仍然20/20存在 type4**。

所以这不是“只要存在 type4 就必须写入”的冗余副本，而是**历史写入端/配置类型
才保留的 MBR 布局快照/底层内容**。

GSerial/BeiZhu 的旧版 NUL 后尾也继续支持同一解释：

- 两盘 GSerial 都是 `"322CA28A\0"`，其后的 `u32@+0x1CA=20417`
  落在条目1 的 `sector_count` 位置；
- 两盘 BeiZhu 都是 GBK `"普通\0"`，NUL 后从 `+0x1D5` 继续出现
  条目2 终点 CHS 尾、`start_lba@+0x1D6` 与 `sector_count@+0x1DA`；
- SanDisk 的条目1/条目2 存续几何与 LBA12 type1/type2 精确一致；
- Aigo 旧 MBR 为连续边界 `entry1 count=20417 -> entry2 start=20480`，
  而 LBA12 新表为 `20418 -> 20481`，呈现明确 ±1 版本差异；type4 边界仍一致。
  这进一步说明旧片段不是由当前 LBA12 简单复制出来，而是独立的旧布局。

写入端/消费端边界也重新核过，并补上 v19.11.4.1 的精确 ABI：

- 当前 Windows `sub_10013FD0` 与 Linux `BuildSector6` 都从静态
  `UsbMainBSec` 复制整扇；当前写入端在写 GSerial/BeiZhu 时，对16B临时缓冲
  先清零，再复制输入槽前15B并固定覆盖16B；
- 历史 v19.11.4.1 `BuildSector6/fcn.10006370` 也先复制完整512B
  `UsbMainBSec@0x101BA790`，但其 BeiZhu 物理槽是更宽的容量=32 ABI：
  `0x10006648..0x10006656` 对 `sector+0x1D0` 调
  `strcpy_s(cap=0x20, caller arg8)`。这意味着旧文档“v19 覆盖项不触及
  +0x1E0..+0x1ED”是错误的；本14B 位于 v19 BeiZhu 槽的**容量范围**内；
- 这仍然不是原始 32B 复制。共享辅助函数 `0x101473FE` 在复制首个 NUL 后立即停止。
  已恢复写入端调用方 `0x1000B16C..0x1000B226` 先把 arg8 的32B局部完整清零，
  再从 `object+0x2620` 以 `strcpy_s(cap=32)` 写入 BeiZhu；同时已固定 v19
  `UsbMainBSec +0x1D0..+0x1F3` 也全部为0。因此短 BeiZhu 下 v19 只能让
  `+0x1E0..+0x1ED` 保持0，不能生成两份物理非零 MBR 底层内容；
- 历史 v19 `ReadSector6@0x10006E07..0x10006E16` 从解码后
  `+0x1D0` 通过同一 `strcpy_s(cap=32)` 返回调用方 arg8，只消费到首NUL。
  六个调用方分三组：第一组目标局部除初始化/两次读取端传参外无引用；第二组
  成功路径读取另一 `esp+0x18` 字符串，不读取目标稳定 `esp+0x38` 槽；第三组
  目标 `-0x64` 的唯一后续值使用是再以 `strcpy_s(cap=16)` 写入对象。
  因而 NUL 后 `+0x10..+0x1D` 没有 cmp/测试/哈希/分支/字段提取业务消费；
- Windows 当前 `UsbMainBSec@0x100E7220` 只发现读取交叉引用，没有运行时写入；
  Linux `UsbMainBSec@0x22BB40` 同样只在当前构造器被读取；
- 当前 `BuildSector0`/Netac/硬件 MBR 构造器都只构造单条普通分区，
  不能生成这里的三分区旧版布局；
- `UDiskLabelRepair.dll` 的 `CLabelRepair::CheckSafe6LabelExist`
  直接从 LBA12 解析 type1/2/4，`Repair0Sector/ReCreate0Sector` 从 sector9/
  备份扇区恢复或重建 LBA0；目前没有发现它直接读取 LBA6 片段。

因此“旧版不透明扩展”这一旧命名仍应撤销。消费端侧现已闭合到
**仅 C 字符串前缀 / NUL 后语义忽略**，但 `+0x1E0..+0x1ED`
仍不能升完全闭环：两份非零物理值确实是旧 MBR 条目3 片段，而 pinned
v19/当前写入端都只能给出全零配置类型。生成该动态旧版 MBR 底层内容的
精确更早写入端/配置类型选择仍缺，故继续保持部分闭环。

新增回归门禁：

- `lba6_legacy_beizhu_post_nul_bytes_continue_into_mbr_type4_fragment`；
- `lba6_authentic_sandisk_legacy_mbr_type4_fragment_matches_lba12`；
- `lba6_legacy_mbr_fragment_is_profile_specific_even_when_lba12_type4_exists`。

第三条门禁明确防止未来把“LBA12 有 type4”错误实现成“新盘必须回填 LBA6 MBR”。

**LBA6 C 字符串槽具有随配置类型变化的 NUL 后保留底层字节**：基于上述写入端、
消费端和22份原始盘反例，本轮纠正之前的严格账本：
`+0x1C0..0x1CF` 与 `+0x1D0..0x1DF` 从完全闭环回退为部分闭环。
这是证据标准收紧后的纠错，不是协议理解退步；两段的 C 字符串业务语义仍然成立。

因此旧免密转换器里的 `0x1CA=128480`、`0x1D4..0x1EC=0`
只能保留为**历史兼容补丁 recipe**，不能再进入新盘制盘的协议模型。
新盘标准配置类型现在按当前写入端边界确定性生成：
`GSerial="322CA28A" + NUL + zero tail`、空 BeiZhu、`0x1E0..0x1EF=0`、
`m_encrypt=1`；不模拟写入端的未初始化尾字节。

#### LBA6 `m_autoid@0x70` / `m_UsbOffice@0x80`：NUL 后保留底层字节来源闭合

Linux DWARF/机器码继续把这条链闭合到整个固定槽的存储行为：

- `UsbLabelParam.m_autoid @ +0x258`；
- `UsbWriteParam.m_autoid @ +0x259`；
- `UsbWriteParam.m_UsbOffice char[64] @ +0x198`；
- `UsbWriteParam(UsbLabelParam&) @ 0x1C362` 分别通过
  `strcpy_s(...,16,...)` / `strcpy_s(...,64,...)` 写这两个数组；
- 该二进制自带的 `strcpy_s(char*, unsigned long, char const*) @ 0x1B9B0`
  逐字节复制，遇到 NUL 后立即返回，**不会清目标缓冲区剩余容量**；
- 这个复制构造器入口也没有先对 0x299B `UsbWriteParam` 整体 memset，
  所以目标数组首个 NUL 后会保留对象原有保留底层字节；
- `BuildSector6@diskfile.cpp:672` 固定 `memcpy 16B` 到 LBA6 `0x70..0x7F`；
- 同一 BuildSector6 固定 `memcpy 64B` 到 LBA6 `0x80..0xBF`；
- `ReadSector6@diskfile.cpp:1005` 再通过 `strcpy_s(...,16,...)` 把
  LBA6 `+0x70` 读回 `UsbLabelParam.m_autoid`；
- 办公同样通过 `strcpy_s(...,64,decoded+0x80)` 读回；
- `BuildSector8` 把同一 `m_autoid` 序列化为 ELABEL `Autonum=`；
- LBA6 SAFE6 校验和覆盖 `+0x000..+0x1FB`，所以 NUL 后保留底层字节
  虽无业务字段语义，仍属于完整性保护的物理存储内容。

全 22 份原始参考只读复核：

- 22/22 的 LBA6 `+0x70` C 字符串与 LBA8 `Autonum=` 完全一致；
- 分布为 `YD000001` 14、空串 6、`1` 2；
- 第一个 NUL 后经常非零；已提交原始样本中**同一个空 autoid**
  至少出现2种不同且非零的 NUL 后保留底层字节；
- 办公同样存在空/非空值，且**同一个空办公**至少出现3种不同且非零的
  NUL 后保留底层字节。

这直接排除了“隐藏字段”与“固定零填充”解释。尾字节值不稳定，但不稳定的
**生成原因与消费规则已经闭合**：复制构造器不预清对象 + 自带 strcpy_s
不清剩余容量 + BuildSector6 固定宽度 memcpy，形成
写入端未初始化保留底层字节；ReadSector6 只解释首个 NUL 前的 C 字符串。

因此严格完全闭环可以覆盖这种“值不确定、行为确定”的存储语义：

- `LBA6 +0x70..+0x7F m_autoid[16]`：16B 部分闭环 -> **完全闭环**；
- `LBA6 +0x80..+0xBF m_UsbOffice[64]`：64B 部分闭环 -> **完全闭环**。

这不要求制盘模拟未初始化内存泄漏；新版磁盘标准可在 NUL 后使用
确定性零保留底层字节，并重算校验和。兼容读取必须继续接受真实旧盘任意保留底层字节。

#### LBA6 `m_usbLabel@0x188`：同一字符串三种非零保留底层字节，56B 整槽闭合

继续沿同一个 `UsbWriteParam(UsbLabelParam&)@0x1C362` 下钻后，标签与
autoid/办公实际共享同一类写入端缺陷：

- Linux DWARF 给出 `UsbLabelParam.m_usbLabel[64]@+0x218` 与
  `UsbWriteParam.m_usbLabel[64]@+0x218`；
- 复制构造器调用
  `strcpy_s(dst+0x218, 64, src+0x218)`；
- 该构造器入口没有先 memset 整个 0x299B `UsbWriteParam`；
- 自带 `strcpy_s@0x1B9B0` 在复制第一个 NUL 后立即返回，不清剩余容量；
- Windows `sub_10013FD0` 与 Linux `BuildSector6@0x1CAAC`
  都只把该64B数组的前 `0x38=56B` 固定复制到 LBA6 `+0x188..+0x1BF`；
- Linux `ReadSector6@0x1E84B..` 从 `decoded+0x188` 构造 C++ 字符串，
  Windows 读取端同构；`BuildSector8` 又把同一个逻辑字符串写成
  ELABEL `Label=`；
- SAFE6 校验和覆盖 LBA6 前508B，所以 NUL 后保留底层字节虽无业务字段语义，
  仍是受完整性保护的真实物理字节。

实盘给出了比“存在非零尾”更强的反例。已提交原始测试夹具中，
业务值全部是同一个 `江苏电力!SAFE6`，但首个 NUL 后的41B 保留底层字节至少出现
**3种不同配置类型，而且3种都含非零字节**。如果这些字节属于隐藏字段或固定填充，
同一业务字符串不应自然产生这种多配置类型；该分布与“不清目标对象 + strcpy_s 到 NUL
即停 + 固定56B memcpy”的官方写入端逐项吻合。

回归 `lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries`
现同时要求：

- LBA6 标签 C 字符串 == LBA8 `Label=`；
- 标签槽存在 NUL 后非零保留底层字节；
- 同一个 `江苏电力!SAFE6` 至少保留3种不同且非零的 NUL 后保留底层字节。

因此 `LBA6 +0x188..+0x1BF` 56B 从部分闭环升 **完全闭环**。这里的完全闭环
表示“业务字符串 + 写入端未初始化保留底层字节”的逐字节行为闭合，不意味着尾部必须
复刻旧内存垃圾；标准新盘仍可在 NUL 后写确定性零并重算校验和，读取端必须接受
旧盘的任意保留底层字节。到这一中间审计阶段，LBA6 更新为
**356 完全闭环 / 156 部分闭环 / 0 未知 = 69.5%**。

#### LBA6 Dept 64B：已知 join59/join60 分叉严格只剩 `+0x3F` 1B

Dept 主槽继续按字节下钻后，不能再把整64B因为旧版 join59 写入端未知而一起留在
部分闭环。Linux `UsbWriteParam(UsbLabelParam&)@0x1C362` 对部门的实际代码是：

`strcpy_s(dst+0x40, 0xBC, src+0x40)`

这与 autoid/办公/标签使用同一个自带 `strcpy_s@0x1B9B0`，并且复制构造器
入口同样不先 memset 0x299B `UsbWriteParam`。因此短 Dept 的物理槽语义已经与其它
固定 C 字符串槽一致：首个 NUL 前是业务字符串，NUL 后是
写入端未初始化保留底层字节。Linux `BuildSector6@0x1CAAC` 又明确分两条：

- `strlen(dept) <= 63`：直接把 `UsbWriteParam+0x40` 的完整64B memcpy 到 LBA6；
- `strlen(dept) > 63`：先清64B临时槽，写标记 `0x40245E2A`，再复制
  Dept 前60B 到标记后，并把 Dept[60..NUL] 写入 LBA9+0x80。

因此当前长配置类型中：

- `+0x00..03 = 2A 5E 24 40`；
- `+0x04..+0x3E = Dept[0..58]`；
- `+0x3F = Dept[59]`。

现有严格原始样本中的当前 Kingston join60 与旧版 Lexar join59
给出一个非常精确的历史边界：两者重建后的完整 Dept 都是同一76B字符串，解密后的
LBA6 Dept 槽 **前63B逐字节完全相同**，唯一差异是最后1B：

- 当前 join60：`+0x3F = 0xA8`，即 Dept[59]；
- 旧版 join59：`+0x3F = 0x00`，LBA9 续段从 Dept[59] 开始；
- 读取端已有明确兼容分支：内联[59] 非零时 join=60，为0时 join=59。

新回归在
`lba9_dept_continuation_preserves_both_official_reader_join_profiles`
中固定“前63B相同、只末1B分叉”；另新增
`lba6_short_dept_slot_is_c_string_plus_uninitialized_backing`，要求至少3份原始
短 Dept 测试夹具的 LBA6 C 字符串与 LBA8 `Dept=` 相同，并且
`+0x00..+0x3E` 内必须保留真实 NUL 后非零保留底层字节反例。

据此可以严格拆分：

- `LBA6 +0x000..+0x03E` 63B：**完全闭环**。短配置类型的
  C 字符串/保留底层字节写入端、长配置类型的标记+Dept[0..58] 写入端、
  读取端、以及当前/旧版原盘均闭合，且已知代际分叉不触及本段；
- `LBA6 +0x03F` 1B：继续 **部分闭环**。当前写入端/读取端已知，
  但旧版 join59 为什么把 Dept[59] 改为NUL、由哪个旧写入端/选择条件产生，
  尚未定位。

这不是把未知旧版写入端“平均摊掉”，而是把它隔离到唯一真实分叉字节。
LBA6 在当时更新为 **431 完全闭环 / 81 部分闭环 / 0 未知 = 84.2%**；后文
长 User 一方运行时闭环后，所有者/User 32B 又从部分闭环升完全闭环，
最终以文末严格总表为准。

#### LBA6 原352B 未知已全部拆清：固定字段槽 + UsbMainBSec 静态模板

本轮从 Windows/Linux 官方 BuildSector6 与 ReadSector6 的真实机器码重新恢复
物理布局，而不是继续把已命名字段之间的空洞记作未知。

Linux DWARF 明确给出 UsbLabelParam(大小=0x2AC) 与
UsbWriteParam(大小=0x299)。与本轮相关的成员为：

- 部门 @ +0x40；
- 所有者 @ +0xFC；
- 办公[64] @ +0x198；
- GSerial[64] @ +0x1D8；
- 标签[64] @ +0x218；
- autoid[16] @ +0x258/+0x259；
- BeiZhu[16] @ +0x268/+0x289。

Windows sub_10013FD0 首先从 UsbMainBSec@0x100E7220 复制整扇到 LBA6；
Linux BuildSector6@0x1CAAC 同样先从 UsbMainBSec@0x22BB40 复制 sector_size。
随后字段覆盖项只覆盖
+0x000..03F、+0x050..06F、+0x070..07F、+0x080..0BF、
+0x100..107、+0x188..1BF、+0x1C0..1CF、+0x1D0..1DF、
+0x1F0..1F3。

因此旧352B 未知可严格拆成两类。

第一类是此前账本漏掉的136B 固定存储槽：

- +0x060..06F：原账本只记录 User 前16B，实际所有者/User 主槽为完整32B
  +0x050..06F；
- +0x080..0BF：m_UsbOffice[64]；
- +0x188..1BF：m_usbLabel 的56B物理主槽。

官方读取端与之对称：所有者普通路径按 C 字符串从解码后+0x50 恢复，
长值使用 0x40245E2A 标记并从 LBA9+0x100 续段重组；
办公从解码后+0x80 读回 64B C 字符串；标签从解码后+0x188
构造字符串后写回 m_usbLabel[64]。

实盘证明三种固定槽都有首个 NUL 后非零保留底层字节。进一步追写入端后，
办公与标签的保留底层字节来源先行闭合；所有者/User 随后也由当前 Windows
一方 `BuildSector6` 最大155B正向运行、`ReadSector6` 动态往返验证以及
`sub_100139F0 -> strcpy_s@0x10097F4F` 的短值保留底层字节生命周期闭合，已升级完全闭环。CI
`lba6_owner_office_and_label_slots_have_official_fixed_storage_boundaries`
继续锁定 LBA6 所有者 C 字符串 == LBA8 User、LBA6 标签 C 字符串 == LBA8 标签，
并同时锁定“同一空办公至少3种保留底层字节”与“同一
`江苏电力!SAFE6` 至少3种不同且非零保留底层字节”两组反例。新增
`official_virtual_long_user_uses_lba6_prefix_and_full_lba9_continuation_slot` 则锁定
最大长 User 盘面配置类型。因此旧136B现在 **所有者 32B + 办公 64B + 标签 56B
全部完全闭环**。

第二类是216B 写入端负责静态 UsbMainBSec 材料：

- +0x040..04F：16B；
- +0x0C0..0FF：64B；
- +0x108..187：128B；
- +0x1F4..1FB：8B。

这216B满足严格完全闭环，而不是因为样本碰巧相同：

1. Windows/Linux BuildSector6 都先复制官方 UsbMainBSec，字段覆盖项
   不触及这四段；
2. Windows sub_100152A0 与 Linux ReadSector6@0x1E2CC 都在任何字段解析前
   对原始 +0x000..1FB 整体计算 SAFE6 校验和，不匹配直接拒绝，因此这216B
   是明确完整性输入；
3. 正确从 ELF .数据的 UsbMainBSec@0x22BB40 抽取后，四段与 Windows
   模板以及实盘逐字节一致；
4. 21份未转换完整备份 + 独立 SanDisk 共22份原始盘全部一致。

+0x108..187 还包含旧版 MBR 的无效分区表 /
原始错误字符串 `Error loading operating system` / `Missing operating system` 的消息材料；
+0x1F4..1FB 是显式模板零。持续集成新增
`lba6_static_usb_main_bsec_holes_are_exact_and_checksum_protected`，
锁定已提交原始样本 + 独立 SanDisk。

此前 LBA6 从 4 完全闭环 / 156 部分闭环 / 352 未知更新为
220 完全闭环 / 292 部分闭环 / 0 未知；随后闭合 autoid 16B + 办公 64B
达到 300/212；随后闭合标签 56B，在继续拆 Dept 之前的中间计数为
**356 完全闭环 / 156 部分闭环 / 0 未知**。

全 LBA0–12 的未知首次降为0B。这只表示每个物理字节至少已有明确区域/
存储行为边界；仍有大量部分闭环尚未满足最终业务语义闭环，不能把
“无未知”写成“协议已经全部完成”。

同时，Dept/所有者溢出路径获得跨扇区纠偏：BuildSector6 的输出指针
指向 LBA6，因此输出 + 3*sector_size + 0x80/0x100 实际落在
LBA9+0x80/+0x100。这给此前 LBA9 Dept/保留底层字节历史配置类型提供了官方
写入端解释，也证明 LBA6/LBA9 不是互不相关的独立扇区。

#### LBA6 `+0x100..0x107`：官方 `m_crcUsbID[2]` 与双倍保护值已恢复

此前这里仅记为“设备 ID CRC材料”，第二 DWORD 没有正式含义。本轮直接用
Linux DWARF、Linux/Windows 写入端、Windows 旧版 check 与严格22份实盘把结构恢复到：

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

写入端不是推测，而是两条 Linux 初始化路径都逐指令一致：

- `CLabelManage::CLabelManage(...) @ 0x1C538`：
  - `memset(this+0x24, 0, 8)`；
  - `CRC32(0, m_strUID.c_str(), m_strUID.length()) -> this+0x24`；
  - `this+0x28 = this+0x24 * 2`；
- `CLabelManage::Init(...) @ 0x1C76E` 重复同一套 `CRC32 -> doubled DWORD` 算法；
- `BuildSector6@diskfile.cpp:710` 执行 `memcpy(out+0x100, this+0x24, 8)`。

Windows 当前写入端与 Linux 独立对齐：

- `sub_10013D20` 清零 `object+0x44..+0x4B`；
- `object+0x44 = CRC32(device-id string)`；
- `object+0x48 = object+0x44 << 1`；
- `sub_10013B80` 的另一条构造路径完全同构；
- `BuildSector6/sub_10013FD0` 执行 `memcpy(out+0x100, object+0x44, 8)`。

第一 DWORD 还是整个标签族的实际密钥源，而不只是“写在 LBA6 里的编号”：

- Linux `BuildSector7` 读取 `this+0x24`，折叠高低16位生成旧版表滚动异或密钥；
- `EncryptSector8Data` 直接把 `this+0x24` 作为4B 密钥；
- `BuildSector12`/`ReadSector12` 也以 `this+0x24` 作为4B加解密密钥；
- `ReadSector8` 两个入口同样从 `this+0x24` 取密钥。

因此 `m_crcUsbID[0]` 可以闭合为**由设备 ID 派生的主标签加解密/滚动密钥材料**。
这里仍然区分“运行时成员的用途”与“LBA6 持久化副本的消费端”：Linux
`ReadSector6` 当前不读取 `+0x100/+0x104`，但在统一完全闭环标准下，
这已经构成该写入端负责副本的 **负语义消费端**，而不是新的语义缺口。
LBA6 前508B 校验和又覆盖这8B，所以物理副本仍属于明确的完整性保护范围。

第二 DWORD 的历史用途也找到了。Windows 当前
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

同一倍增判断还复制在 `cemsudisk` 与 `vrvaud_c` 的 SAFE6 解析器中，说明
`m_crcUsbID[1]` 的确是历史上的 **双倍 CRC 一致性保护值**，不是随机派生值。

但本轮同时回到 PE 机器码复核可达性，避免把反编译伪代码误当当前消费端。
`cemsusbregsiter.dll` 在该判断前实际是：

```text
mov edx, 1
test edx, edx
je   legacy_crc_pair_check
```

因此当前 binary 永远走 `if(1)` 的正常解析分支，CRC pair check 不可达；
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

按当前仓库已经用于 `m_encrypt`、兼容槽等字段的统一标准，
这8B现在可以完成闭合：

- `+0x100..103`：完全闭环，写入端负责身份/密钥元数据。正式字段名、
  双平台写入端、CRC32(device_id) 算法、同源运行时密钥用途、LBA6
  负语义消费端、校验和所有权与22/22实盘均齐全；
- `+0x104..107`：完全闭环，保留的双 CRC 兼容校验值。正式
  写入端与历史双倍保护值消费端都已存在；当前构建虽不走
  历史分支，但正常读取端明确语义忽略该副本，校验和仍覆盖；
  22/22实盘严格满足倍增关系。

这不是把不可达死代码当当前消费端；完全闭环的对象是**盘面字段的完整生命周期**：
当前写入端继续写、当前读取端可忽略、历史读取端曾按双倍保护值消费。
  但当前三个同源实现中的该判断均不可达。

禁止后续仅凭死代码或“同源成员用于其它扇区加密”把这8B升级完全闭环。

#### LBA8：加密长度由 LLGB +0x04 决定，不是固定 368B

- 当前 22/22 参考样本均满足：
  - 解密后 `0x00..0x03 == "LLGB"`；
  - `u32@+0x04 = 0x80 + strlen(ELABEL)`，**不包含结尾 NUL**；
  - 实际 A6B0/A7F0 长度为
    `((u32@+0x04 / 16) + 1) * 16`，即始终覆盖装有结尾 NUL 的下一块；
  - 当前 22 份原始参考在该动态加密长度之后都观测为物理零；但这只是样本事实，
    **不是写入端的全零填充语义**。Windows/Linux BuildSector8 都只覆盖并
    加密动态前缀，不会清零输出扇区剩余尾部；当前注册路径又以预读的既有
    LBA0–12 缓冲为保留底层字节，因此 `encrypted_len..` 的正式行为是
    **原样保留现有物理字节**；
- 真实样本的有效长度覆盖 `0x148 / 0x154 / 0x16b / 0x17a / 0x17c / 0x17e / 0x181 / 0x183` 等多种值，实际加密前缀可为 0x150、0x160、0x170、0x180、0x190。
- 独立 SanDisk 原始加密盘此前曾被误判为“非 LLGB”：根因是使用了
  `disk&ven_sandisk&prod_ultra&rev_1.00` 这一另一份免密/历史样本的短 device_id。
  该原盘自己的 LBA7 只有在
  `disk&ven_sandisk&prod_ultra_usb_3.0&rev_1.00` 下才能恢复 EDPF；
  同一权威 device_id 解 LBA8 后得到标准 `LLGB`，`+0x04=0x15E`。
  因此当前原始参考集是 **22/22 LLGB**，没有证据把该 SanDisk 归入 EKTF。
- 因此旧固定 368B (`0x170`) 解码器会：
  - 对短标签多解无意义块；
  - 对长标签截断真实 `VOL/VOLC` 字段。
- 本轮又重新按严格22份原始生成参考逐盘复算：
  - `logical_end` 范围为 **0x148..0x183**；
  - 实际加密前缀范围为 **0x150..0x190**；
  - 22/22 的物理尾部当前为零，但写入端明确允许已有非零保留底层字节被保留。
  为防止实现重新把尾部当作 LLGB 密文或固定零区，
  `tests/inspect.rs::lba8_preserves_nonzero_bytes_after_the_dynamic_encrypted_prefix`
  构造非零物理尾部，要求检查只解密动态前缀并原样保留后部字节。
  同时新增
  `lba8_decrypts_one_extra_block_when_logical_length_is_16_byte_aligned`：
  旧检查使用普通 `round_up_16(logical_len)`，当 logical_len 恰好16B对齐时
  会少解一整块；现已按官方写入端的
  `(logical_len / 16 + 1) * 16` 修正，明确把 ELABEL 结尾 NUL 所在的额外块纳入
  A6B0 解密，并继续保持其后物理保留底层字节不动。
  因而旧账本按当前样本最大正文位置切出的 **102B 未知** 已撤销：
  `+0x80..+0x1FF` 必须作为一个动态的
  **ELABEL / 加密块填充 / 保留尾部** 区整体记为部分闭环。
  LBA8 当时严格状态同步纠正为 **86 完全闭环 / 426 部分闭环 / 0 未知**；
  后续再把头 `MacInfo[6]` 独立闭合后，当前为
  **92 完全闭环 / 420 部分闭环 / 0 未知**。
- LBA8 `+0x14..+0x17` 与 LBA4 解密后的 `0x35..0x38` 在当前 22/22 参考样本逐字节一致，是跨扇区动态字段，不是可固定配置类型常量。

##### LBA8 当前 UsbOnlyInfo：主 onlyid 的十六进制盘面表达

本轮继续把 `+0x14..+0x3D` 的当前写入端从“格式看起来像 onlyid”
追到 Windows 实际调用栈，避免用样本相关性代替写入端证据。

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

进入 `sub_100148d0` 后，位于 按值 `UsbLabelParam` 之后的尾随 DWORD
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

因此当前 Windows 写入端可以严格写成：

```text
UsbOnlyInfo = sprintf("%08x%08x", main_onlyid_bits, 0)
```

Linux `CLabelManage::BuildSector8(char*, UsbLabelParam, unsigned int)` 也以同一
`"%08x%08x"` 模板消费第三个 u32 参数，形成跨平台写入端对照。

严格 22 份原始生成参考重新按已经独立闭合的 LBA4 身份配置类型分组：

- **6/6 当前身份**：
  - `OnllyID2Nd == main onlyid`；
  - `HSerialCRC[5] == 0`；
  - LBA8 `HDSerialInfo == 0`；
  - `MacInfo[6] == 0`；
  - `UsbOnlyInfo == format("%08x%08x", main_onlyid_bits, 0)`；
- **16/16 旧版身份**：
  - `UsbOnlyInfo[32]` 为空/全零；
  - `HDSerialInfo` 保留历史非零配置类型；
  - 独立 SanDisk 原始盘也落在该旧版组，没有当前规则误命中。

CI 新增
`lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty`
锁定当前/旧版双配置类型，防止以后把二者错误归一。

这里随后进一步按正式结构边界拆分，而不是继续把42B绑在一起：

- `HDSerialInfo@+0x14..+0x17`：当前=0，但16份旧版存在非零配置类型，
  旧写入端未定位，继续部分闭环；
- `MacInfo[6]@+0x18..+0x1D`：Linux DWARF正式命名，Windows/Linux 写入端
  都从全零头初始化得到6B零；语义读取端只从 ElabOffset 进入 ELABEL，
  不读取 MacInfo；严格22份跨当前/旧版 **22/22均为6B零**，没有已知
  配置类型分叉。因此这6B满足写入端 + 负向消费端 + 真实设备
  严格标准，升级 **完全闭环**；
- `UsbOnlyInfo[32]@+0x1E..+0x3D`：当前写入端已闭合，
  但16份旧版均为空且旧写入端/最终历史消费端未闭合，继续部分闭环。

CI 的
`lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty`
现在对所有已提交原始配置类型统一断言 `MacInfo[6]==zero[6]`，
不再只在当前分支检查。

动态头本轮继续从写入端/消费端重新核对，新增闭合 76B：

- `+0x08..0x0B ToolVersion[4]`
  - Windows `sub_100148d0` 和 Linux `BuildSector8@0x1D602`
    都固定写 `01 00 00 01`；
  - 22/22 原始盘一致；
- `+0x0C..0x0F Labversion`
  - 两端写入端均固定写 `0x222`；
  - 22/22 原始盘一致；
- `+0x10..0x13 writeTime`
  - Windows `sub_10016610` 直接返回 `GetTickCount()`；
  - Linux `CLabelManage::GetTickCount@0x1FBAA` 用
    `CLOCK_MONOTONIC` 计算毫秒并返回低32位；
  - 22/22 原始盘均非零并存在跨标签多值，因此不能再误称墙钟时间戳；
- `+0x40..0x7F Reserverd[64]`
  - 两端写入端都先零初始化完整临时头，该64B没有后续赋值；
  - 22/22 原始盘解密后全零。

消费端同样闭合：

- `ReadSector8(char*, UsbLabelParam&)` 只检查魔数、读取
  `ElabOffset` 并解析 ELABEL，跳过上述四段；
- `ReadSector8(char*, BYTE*)` 只在 LLGB 校验通过后原样复制整份
  512B 解密结果，并返回 `cbSize`，不解释这些字段。

因此这 76B 具备写入端 + 消费端行为 + 22盘原始证据，可以升级完全闭环。
但 `+0x14..0x3D` 仍明确保持部分闭环：当前写入端的
`HDSerialInfo/MacInfo/UsbOnlyInfo` 写法无法解释所有历史盘，不能被相邻闭合字段带着升级。

Windows `sub_100148d0` 与 Linux
`CLabelManage::BuildSector8(char*, UsbLabelParam, unsigned int)`
还独立给出同一份 17 个键 ELABEL 写入端模板：

`<ELABEL>GLab=%s||Indus=%s||Orgcd=%s||Org=%s||Unit=%s||Dept=%s||User=%s||Alarm=%s||Autonum=%s||Label=%s||Rmark=%s||VOL0=%s||VOL1=%s||VOL2=%s||VOLC0=%s||VOLC1=%s||VOLC2=%s||`

Linux 写入端与 `UsbLabelParam` DWARF 对齐后，当前配置类型的赋值来源是：

- GLab <- `m_usbGSerial`
- Dept <- `m_usbdepartment`
- User <- `m_usbowner`
- Autonum <- `m_autoid`
- 标签 <- `m_usbLabel`
- Rmark <- `BeiZhu`
- 字段 `Indus` / `Orgcd` / `Org` / `Unit` / `Alarm` / `VOL0/1/2` / `VOLC0/1/2`
  在该写入端中保持初始空值。

22 份原始参考样本按**原始字节先切 `||`、再逐值做 GBK**
重新统计后，17 个密钥在 22/22 中全部存在：

- GLab：22/22 = `322CA28A-D7D1448B-DCE2CED9`
- 标签：22/22 = `江苏电力!SAFE6`
- 字段 `Indus` / `Orgcd` / `Org` / `Unit` / `Alarm` / `VOL0/1/2` / `VOLC0/1/2`：22/22 空
- Autonum：`YD000001` 14 份、空 6 份、`1` 2 份
- Rmark：空 20 份、`普通` 2 份
- Dept / User 为业务动态值。

其中 4 份较长盐城 Dept 的原始值最后停在单个 GBK 前导字节 `0xBD`，
紧接 ASCII `||User=`。这证明解析器必须**先按原始 ASCII delimiter
切字段，再分别解码字段值**；若先整段 GBK 解码，`0xBD 0x7C`
会吞掉第一个 `|`，破坏 User 字段边界。edpcli 已增加该实盘形态的回归测试。

制盘也已按官方写入端修正：

- 不再限制“LBA8 正文 <= 240B / 固定加密 0x170”；
- 标准 ELABEL 在扇区容量范围内动态生成；
- `+0x04` 不包含 NUL；
- 若 `+0x04` 恰为 16B 整数倍，仍额外加密一个块以容纳结尾 NUL。

##### LBA8 `+0x080..0x1FF`：动态 ELABEL / 加密后的保留底层字节 / 保留尾部完整闭合

继续回到两套官方写入端的机器码后，原先“加密块填充”这一命名必须纠正。
Windows `cemsusbregsiter.dll::sub_100148d0` 与 Linux
`CLabelManage::BuildSector8@0x1D602` 的行为完全同构：

1. 写入端自己只清零局部工作区，不清调用方提供的 512B LBA8 输出；
2. 把 17 个键 ELABEL 连同终止 NUL 写到 `ElabOffset=0x80`；
3. 以 `(logical_len / 16 + 1) * 16` 计算 `encrypted_len`；
4. 直接对调用方输出的 `0..encrypted_len` 原地加密；
5. `encrypted_len..0x1FF` 完全不触碰。

Windows 主注册链又给出保留底层字节来源的独立闭环：`RegsiterUsb` 先用
`ReadSectorData(..., count=0x0D)` 将旧 LBA0–12 读入 `var_500`，随后直接把
`var_500 + 8*sector_size` 传给 `sub_100148d0`。因此：

- ELABEL 正文与终止 NUL 是写入端负责；
- **ELABEL NUL 后到 encrypted_len 的字节属于既有保留底层字节**，只是因为位于动态加密前缀内而被一起变换，不是协议全零填充；
- `encrypted_len..0x1FF` 是未加密的原样保留现有物理尾部。

消费端需要分两层看。注册/制标侧 Windows `cemsusbregsiter.dll::sub_10015820` 与
Linux `ReadSector8(UsbLabelParam&)` 都只执行同一组7键回填：

`registration semantic reader key set = Label/GLab/Dept/User/Autonum/Rmark/Unit`

Linux 读取端分别回填 `UsbLabelParam.m_usbLabel/m_usbGSerial/m_usbdepartment/`
`m_usbowner/m_autoid/BeiZhu/m_usbUnit`。但继续审计运行时 DLL 后发现此前“其它十键无
消费端”的结论过强：`out_raw_data/EdpEDiskCtrl.dll::sub_10016260` 是完整的运行时
读取端，**运行时 EdpEDiskCtrl 读取端解析全部 17 个 ELABEL 键**，并将
`GLab/Indus/Orgcd/Org/Unit/Dept/User/Alarm/Autonum/Label/Rmark/VOL0/VOL1/VOL2/`
`VOLC0/VOLC1/VOLC2` 全部解析到 `tagEdpUsbLableInfo` 的固定槽。因此十个当前空值键
应称兼容盘面槽，而不能称为“全产品负向消费端”。

实盘侧，严格22份原始参考的正文 22/22 都保持相同17 个键顺序；十个当前兼容密钥 22/22 均为空。已提交原始样本新增
`lba8_real_elabel_keeps_all_wire_keys_and_current_compat_slots_empty`，锁定多种
`logical_len`、完整17 个键顺序、十个兼容槽空值以及当前块内保留底层字节的零观测。
另新增 `lba8_preserves_nonzero_backing_inside_the_last_encrypted_block` 合成回归，明确证明
检查对 NUL 后但仍位于加密前缀内的非零保留底层字节会正确解密保留；此前已有
`lba8_preserves_nonzero_bytes_after_the_dynamic_encrypted_prefix` 与
`lba8_decrypts_one_extra_block_when_logical_length_is_16_byte_aligned` 分别锁定块外尾部
原样保留和 16B 对齐额外一块。

因此 `+0x080..0x1FF` 的384B不再存在未闭合的存储/消费状态：同一偏移可随
`logical_len/encrypted_len` 动态属于“正文+NUL / 加密保留保留底层字节 / 未加密
保留尾部”，但三类边界、写入端所有权、消费端行为和原始实盘配置类型都已闭合。
本段从部分闭环升 **完全闭环**。这里的完全闭环绝不意味着保留底层字节/尾部必须为零；
未来遇到非零旧配置类型必须按动态边界保留。LBA8 于是从 `92/420` 提升到
**476 完全闭环 / 36 部分闭环 / 0 未知 = 93.0%**。

继续追运行时读取端还补上了剩余头的结构传递链：`sub_10016260` 不仅解析17键，
还明确复制 `HDSerialInfo@+0x14` 和 `UsbOnlyInfo[32]@+0x1E` 到输出
`tagEdpUsbLableInfo`；其隐藏输出指针位于调用帧 `+0x120`。上层
`sub_100167c0` 的第6参数就是该输出指针，`CEdpDiskControl::UpdateLabelInfo` 传入
`this+0x2D0`，所以两字段最终缓存于对象 `+0x2E4` 与 `+0x2EE..+0x30D`。目前对这些
精确对象偏移尚未找到值相关行为读点；因此这只能补强“正式运行时结构消费端”，不能替代
旧版写入端/派生公式与最终行为语义。剩余36B仍保持部分闭环。

继续向旧版本追溯后，`HDSerialInfo` 的官方写入端家族已从“完全未知”推进到可执行算法，
但还不足以跨过严格完全闭环门槛。新增证据如下：

- 从金山公开 DLL 档案取得 2020 `CEMSUsbRegsiter.dll` v19.11.4.1，MD5
  `783d01f19e998a514834bc5e5f4249ad`。其 ELABEL 模板在 `sub_10007DF0` 有真实交叉引用，
  不是链接残留；函数先清零 0xD54 临时标签结构，再调用 `UsbTools.dll` ordinal4，若结果为0
  才回退 ordinal3，并把结果DWORD写入 LLGB 头 `+0x14 HDSerialInfo`。
- 同一写入端随后直接在 `+0x1E UsbOnlyInfo[32]` 上执行
  `wsprintf("%08x%08x", caller_dword, hd_serial_info)`；因此 2020 这一代明确存在
  **HDSerialInfo非零 + UsbOnlyInfo第二DWORD复写同一值** 的过渡配置类型。
- 本机 `UsbTools.dll` 的导出表把 **ordinal4 精确命名为 `EDP_DiskNumber`、ordinal3
  精确命名为 `EDP_DeviceNumber`**；两个导出又分别是纯 thunk，跳到
  `DeviceNumber.dll` 的 ordinal3 / ordinal1。这里必须区分两层 DLL 的序号：
  `UsbTools` 是4/3，`DeviceNumber` 自己是3/1，旧文档把“语义映射”和“序号编号”
  混成一句的写法已纠正。两代 `EDP_DiskNumber` 的机器码同构：
  枚举 `PhysicalDrive0..3`，通过 `SMART_RCV_DRIVE_DATA(0x7C088)` 下发 ATA
  `IDENTIFY DEVICE(0xEC)`，取 words10..19 的20B 序列号编号；每16 位字交换字节、
  裁剪首尾ASCII空格，读取失败或20B全零序列号跳过，其余按物理盘序号**无分隔拼接**。
  最终以标准反射形式 CRC32 polynomial `0xEDB88320`、初始值=0 对完整拼接字节串求值。
  因而已知 ordinal4 主路径可写成：

```text
EDP_DiskNumber = CRC32(serial_PhysicalDrive0 || serial_PhysicalDrive1 || ...)
```

  这里每个 `serial` 都是上述 ATA 规范化后的 C 字符串，失败/全零盘不参与。
- ordinal3 回退 `EDP_DeviceNumber` 也已继续闭合到可执行公式。2008
  `DeviceNumber.dll::EDP_DeviceNumber@0x10011E00` 使用同一个 stringstream 和同一个
  CRC32 辅助函数：每个有效物理盘的规范化 ATA 序列号仍按盘号无分隔写入主流；
  随后调用 `fcn_10014120(out, 1)` 获取 MAC 身份串，并通过
  `0x10010E70 = ostream << std::string` 把整串原样追加。模式=1 的跳表只走 MAC
  分支，不序列化 IP：适配器描述先转大写，VMware 虚拟适配器由同时命中
  `VIRTUAL` 与 `VMWARE` 的路径排除，MAC 全零也跳过；接受的6B MAC 用格式化器
  风格=4 输出为12位**大写、无冒号、无连字符**十六进制。序列化格式为：

```text
MACAddress0=AABBCCDDEEFF\r\n
MACAddress1=001122334455\r\n
...
MACCount=N\r\n
```

  索引从0递增；没有有效 MAC 时该辅助函数返回空串。因此回退可写成：

```text
EDP_DeviceNumber = CRC32(
    normalized_ATA_serials_without_delimiters ||
    MACAddress_lines || "MACCount=" || decimal(N) || "\r\n"
)
```

  CRC 仍是反射形式 poly `0xEDB88320`、初始值=0。`0x10010C60` 已由实现行为锁为
  `ostream << const char*`，`0x10010E70` 为 `ostream << std::string`，而整数索引/数量
  通过 `0x10011250` 写入并以 `ret 4` 消费参数；所以这不是根据字符串常量猜出的模板，
  而是已沿调用栈闭合的真实序列化顺序。
- 另取得并核验 2020 `EdpEDiskCtrl.dll` v3.6.10.18，MD5
  `95a06e0d466ba40a7d5c0e6a409e2114`。其 `ReadOrgInfoSector@0x1000C870`
  已经完整复制 `HDSerialInfo@+0x14` 与32B `UsbOnlyInfo@+0x1E`，反证“旧读取端只有4B
  UsbOnlyInfo”的早期猜测。该 DLL 不导入 `DeviceNumber.dll`；围绕 `this+0x1728` 的后续
  直接成员 审计只消费 `+0x00..+0x0C` 一带版本/标志。全DLL唯一
  `this+0x173C` 命中位于 `0x10015CA7`，用途是把该地址作为0x104B路径缓冲区覆写、规范化并
  `CreateFileA`，而不是读取原来的 HDSerialInfo DWORD。其它已确认结构搬运也止于 `+0x0D`，
  没有把 `+0x14` 间接搬去做比较。
- 已提交原始回归 `lba8_current_usb_only_info_is_main_onlyid_hex_while_legacy_profile_keeps_it_empty`
  现额外锁定旧版分支 `HDSerialInfo != 0`；当前仍锁定 `HDSerialInfo == 0`。

因此当前最严谨的代际模型至少是三段：

1. 严格旧版原始样本：`HDSerialInfo != 0`，`UsbOnlyInfo[32] == 0`；
2. 2020 官方过渡写入端：`HDSerialInfo = EDP_DiskNumber`（0时回退
   `EDP_DeviceNumber`），`UsbOnlyInfo` 第二DWORD复制同一值；
3. 当前写入端：`HDSerialInfo = 0`，`UsbOnlyInfo = main_onlyid || 0` 的16字符十六进制文本。

这批证据闭合了一个真实非零写入端家族、`EDP_DiskNumber` 的主算法以及 2020 运行时的
负向值消费端行为，但**尚未找到能生成严格旧版第1种组合的更早精确写入端**，
ordinal3 回退 `EDP_DeviceNumber` 的完整输入公式现已补齐；但**尚未找到能生成严格
旧版第1种组合的更早精确写入端 / 配置类型选择**。不过继续把 `UsbOnlyInfo[32]`
按真正发生代际分叉的位置拆开后，后16B可以独立闭合：当前 Windows/Linux 和2020
过渡写入端都只生成固定16字符 `%08x%08x`，且目标头/临时结构预先整体清零，
因此 `+0x2E` 必为C 字符串终止NUL、`+0x2F..+0x3D` 15B保持零；严格旧版整槽缺失/全零，
同一16B自然也是零。注册语义读取端跳过整槽，2020/当前运行时只结构保存；
另一本机 v3.6.12.28 `EdpEDiskCtrl` 分支甚至只复制 `+0x1E` 首DWORD，直接跳过后28B，继续
证明后缀不是行为字段。已提交严格原始样本新增统一门禁锁定22/22 后缀=`zero[16]`。

因此 LBA8 进一步拆为：

- `HDSerialInfo@+0x14..+0x17`：4B 部分闭环；
- `UsbOnlyInfo text@+0x1E..+0x2D`：16B 部分闭环，三代配置类型真实分叉仍需追；
- `UsbOnlyInfo suffix@+0x2E..+0x3D`：**16B 完全闭环**，语义为固定终止NUL+全零后缀。

LBA8 由 **476 完全闭环 / 36 部分闭环** 提升到 **492 完全闭环 / 20 部分闭环**。

#### LBA11：`DRKB + random252`，VID/PID 是 4 字符 ASCII

- `0x000..0x003 == "DRKB"`：当前 22/22。
- `0x004..0x0ff`：252B 运行时随机材料。
- Linux 官方 DWARF 已恢复写入端 / 消费端的原始源码位置：
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
  写 `rand()%255`。因此前 256B 不再是“未知随机数据块”，而是
  **4B 协议魔数 + 252B 明确 PRNG 输出**。
- `ReadSector11` 会先检查这 4B `DRKB`，随后把完整前 256B
  原样作为后半区密钥派生输入。因此 random252 的**生成来源和消费用途都闭合**。
- CRC 输入为：

  `DRKB || random252 || VID_ascii4 || PID_ascii4 || size_le64`

- VID/PID 按备份文件中的四位十六进制 ASCII 文本参与 CRC；将 VID/PID 当作数值小端，当前 22 份参考样本均不能解出 PDKB。
- 后半 `0x100..0x1ff` 用该 CRC 的 4B 小端作为 A6B0 密钥；解密后当前 22/22 均为：

  `PDKB || device_id || 0x00 || zero_padding`

- Linux `BuildSector11` 明确先写 `PDKB`，再把
  `CLabelManage::m_strUID` 复制到 `+0x04`；`m_strUID`
  来源于构造参数 `pUID`。对应消费端解密后检查 `PDKB`，
  再把 `+0x04` C 字符串赋给输出 `strDPBack`。
  当前 21 份完整原始备份已逐份复核：
  **21/21 PDKB 字符串精确等于各自 device_id**；独立 SanDisk
  已在此前 22 份总审计中独立闭合。
- Windows 侧存在第二套独立同构实现：
  - 写入端 `cemsusbregsiter.dll::sub_10014720 @0x10014720`
    （反编译文件约 L78343）；
  - 随机写入端 `sub_10002B90 @0x10002B90`（约 L82081）；
  - KDF/加密 `sub_10002C30 @0x10002C30`（约 L82112）；
  - 消费端 `sub_10015F00 @0x10015F00`（约 L93391）。
  它们分别检查/写入同样的 `DRKB/PDKB`，并使用同一
  `rand256 || VID4 || PID4 || size8 -> CRC32 -> cipher` 公式。
- 当前 Windows 写入端的 `size8` 来源也已闭合到物理容量：
  `sub_10019780` 打开 `\\.\PHYSICALDRIVE%d`，调用
  `DeviceIoControl(..., 0x700A0, ..., 0x28)`，把返回
  `DISK_GEOMETRY_EX +0x18` 的 64-位 `DiskSize` 写进磁盘信息对象
  `+0xB0/+0xB4`；`RegsiterUsb` 复制同一字段并传给
  `sub_10014720`。
- 本轮把当前 Windows 的这条大小链进一步追到逐次对象复制，排除了
  “枚举得到 DiskSize 后又在中间层做 CHS 转换”的可能：
  1. `sub_100186C0` 枚举 USB 磁盘时在栈上构造磁盘信息对象
     `var_7E0`（机器码 `1001878C: lea -0x7E0(%ebp)`）；
  2. `sub_10019780(...,&var_814)` 成功后，机器码
     `10018940..10018952` 直接把这 64-位值写到
     `var_7E0+0xB0/+0xB4`（即 `ebp-0x730/-0x72C`）；
  3. `sub_10019270` 从枚举集合选中目标磁盘；
  4. `sub_100196D0` 机器码 `10019731..10019741` 把选中对象复制进
     `CUsbRegsiter+0x808` 包装对象内部的 `+0x10` 磁盘信息子对象；
  5. `RegsiterUsb` 调 `sub_10018480` 再把该 `+0x10` 子对象复制到
     栈上 `var_158`；`sub_10017D10/sub_10017940` 对
     `+0xB0/+0xB4` 是 DWORD 原样复制；
  6. `var_158` 基址为 `ebp-0x158`，所以
     `+0xB0/+0xB4 == ebp-0xA8/-0xA4`；机器码
     `1003BD0A..1003BD17` 正是把这两个 DWORD 原样 push 给
     `sub_10014720(BuildSector11)`。
  当前写入端从 IOCTL 的 `DiskSize` 到 LBA11 KDF 输入之间没有任何
  `255*63*512` 取整。
- 历史兼容公式本身也已经从“疑似 CHS 辅助函数”升级为**官方命名证据**：
  Linux `libcemsfilesyscheck.so` 带 DWARF 的
  `CDisk::GetWindowsDiskSizeFromLinux(unsigned long long&) @0x186D0`
  对应 `DiskInterface.cpp:1196`；独立 `checkdiskback` 中同名函数
  `@0x413040` 使用同一公式。两者都把容量按
  `255*63*512 = 0x7D8200` 向下取整。Windows
  `sub_100184C0` 是同构实现。
- `cemsusbregsiter.dll::sub_100184C0` 在当前注册构建中确实没有业务调用方，
  不能用它解释实盘；但本轮从独立官方 `UDiskLabelRepair.dll` 找到真正有效的
  CHS 写入端/读取端链：`CLabelRepair::Repair -> sub_10008820(Check LBA11)`，
  校验失败后进入 `sub_10008950(ReWrite11Sector) -> sub_10003A40` 重建 LBA11。
- 修复的物理盘对象由 `sub_10002A20 -> sub_10002FF0` 初始化；后者优先调用
  `DeviceIoControl(IOCTL_DISK_GET_DRIVE_GEOMETRY=0x70000, out=0x18)`，得到标准
  `DISK_GEOMETRY`。`sub_10002A20` 再通过 `sub_10019EF0` 计算
  `Cylinders * TracksPerCylinder * SectorsPerTrack * BytesPerSector`，把64位结果写到
  `disk_info+0x30/+0x34`。`sub_10019EF0` 机器码已复核为标准64位乘法辅助函数。
- `sub_10008820 -> sub_10003BD0` 用同一 `disk_info+0x30/+0x34` 作为 LBA11 KDF
  容量输入；`sub_10008950 -> sub_10003A40` 重写时也传同一64位 CHS 容量。
  因此修复路径同时给出了 CHS 配置类型的有效消费端与写入端。
- 严格22份原始生成参考仍是 **21/22 精确 DiskSize、1/22 CHS-floor**；
  唯一 CHS 参考为 Aigo U335 `onlyid=1987718388`。但本轮新增两个反例把
  “CHS 由设备型号/配置类型静态决定”的解释排除了：
  - 独立原始 SanDisk（HSerial 高熵、LBA6 `+0x1E0..1EF` 非零）
    的 LBA11 仍只在精确 DiskSize 下恢复 `PDKB + device_id`；
  - 同一 Aigo U335、同一 `device_id/VID/PID/物理容量` 的辅助真实采集
    `LBA11.bin/-3/-4` 使用 CHS，而 `LBA11-2.bin` 使用精确 DiskSize。
    这批辅助文件不计入22份代际参考，只用于证明
    `rev_pmap` 字样/硬件身份本身不能唯一决定大小配置类型。
- 因而配置类型选择现已闭合到**调用路径**而不是硬件属性：
  **正常注册写入端/读取端 = 精确 `DISK_GEOMETRY_EX.DiskSize`；修复
  写入端/读取端 = 传统 `DISK_GEOMETRY` CHS 容量。**
  同一 Aigo U335 rev_pmap 的 CHS/精确双真实捕获正好对应这两条官方路径。
- 新增回归门禁：
  - `lba11_authentic_sandisk_legacy_profile_still_uses_exact_disk_size`；
  - `lba11_same_rev_pmap_device_has_both_chs_and_exact_size_writer_profiles`。
  第二条测试使用的 `LBA11-2.bin` 被明确标注为辅助行为证据，不进入22份
  原始代际参考统计。
- 严格完成统计因此更新为：
  - `0x000..0x0FF`：**256B 完成**；
  - `0x100..0x103`：**4B 完成**；
  - `0x104..0x1FF`：**252B 完成**；
  - **LBA11 = 512B 完全闭环 / 0B 部分闭环 / 0B 未知。**
- 制盘熵已同步改成仅接受 `random252`；构造器自己写
  `DRKB`，调用方不再能够把前 4B 协议结构字节当成外部随机材料。

#### LBA12：整扇 512B 是一个连续 A6B0/A7F0 密文

- 对当前 22/22 参考样本直接执行 `a6b0_full(raw512, CRC32(device_id), counter=0)`：
  - `decoded[0..4] == "EDPF"`；
  - `decoded[0x170..0x200] == zero[144]`。
- 因此旧描述“只加密前 368B，后 144B 原始”错误。
- 过去观察到的 `raw[0x170..] == a7f0_full(zero144, key, initial_counter=0x170)`，正是“整扇连续加密”的自然结果，不是独立尾部格式。
- `0x170` 的正确含义只是 **当前 LBA12 EDPF 主表/表尾区域的结束位置**，
  不是密码学边界。代码常量已从误导性的 `EDPF_ENC_LEN` 改为
  `EDPF_TABLE_LEN`。
- edpcli 当前实现已统一为：
  - 检查：整扇 512B 解密，展示时只在 `0x000..0x16F` 解析 EDPF 结构；
  - 旧盘 `convert_lba12`：整扇解密，只修改 EDPF 条目，再整扇重加密；
  - 制盘构造器：构造 512B 明文后一次性整扇加密；
  - 制盘校验器：整扇解密，校验表区，并要求标准配置类型的
    `0x170..0x1FF` 明文为零。
- 旧盘转换 golden 14/14 在改成整扇重加密后哈希完全不变。原因是
  `0x170..0x1FF` 明文未修改，且 A6B0 是确定性的连续计数器模式；
  因而重新计算得到的尾部密文与旧实现“直接拼回原密文尾部”逐字节相同。
  这说明实现修正没有改变既有产品输出，只消除了错误协议模型。

本轮又专门复核了 `0x12E..0x1FF` 的写入端/消费端边界，用来解决
逐字节主表与总进度表之间的旧账本不一致：

- Windows 当前写入端 `sub_10014F30`：
  - 分配 `sector_size+1`；
  - 对整个缓冲执行 `memset(...,0,sector_size+1)`；
  - v0x206 只复制 `0x120` 的 3×96B 紧凑布局条目，再复制 `0x0E` 密码信息；
  - 即最后一次结构写入恰好结束于 `0x12E`；
  - 后续对整扇执行加密并复制回输出；
- Windows 当前读取端 `sub_100160B0`：
  - 固定对完整 `0x200` 字节解密；
  - 魔数/版本合法后只复制 `0x120` 条目区与 `0x0E` 密码信息；
  - `0x12E..0x1FF` 没有结构读取或返回；
- Linux `CLabelManage::BuildSector12@diskfile.cpp:922` 独立给出同样的生成原则：
  先按 `sector_size+1` 分配并整块清零，之后只复制相应 ABI 的表和表尾，
  最终以 `m_nSectorSize` 对整个扇区加密；
- Linux ABI 的 v0x206 表本身是 104B×3 的扩展布局，结束位置不同，
  因此它只用于证明“整扇先零初始化、未写区域保持零并整扇加密”这一写入端
  原则；**不能**拿 Linux 的 `0x138+0x0E` 偏移反向覆盖 Windows 96B 紧凑布局主盘面；
- Windows 紧凑布局主盘面的精确边界仍由 `sub_10014F30/sub_100160B0`
  锁定为 `0x12E`。

实盘门禁也从原先只看 `0x170..` 收紧为完整表后区：

- 已提交原始测试夹具全部满足解密后 `0x12E..0x1FF == zero[210]`；
- 独立 SanDisk 原始也满足同一条件；
- 新门禁 `lba12_post_table_plaintext_is_zero_through_sector_end` 锁定完整210B；
- 原 `every_committed_lba12_tail_is_encrypted_zeroes_from_device_id` 继续单独锁
  `0x170..` 的连续计数器密文性质，防止未来再次把它误改成原始尾部。

因此 `0x12E..0x1FF` 应统一记为 **210B 完全闭环表后全零填充**。
此前字段主表把 `0x170..0x1FF` 仍写成部分闭环是陈旧状态；但下方严格进度表
和 LBA12 的 `393B COMPLETE` 早已把这144B包含进去，所以本轮只是修正字段账本，
**不得再次把144B加到总完全闭环数**。

#### onlyid：注册时随机 GUID 的 CRC32，不是硬件 ID

- Windows 官方注册链：

  `CoCreateGuid -> GUID raw 16B -> CRC32_bare -> object+0x698 -> LBA4 $$$onlyid$$$`

- 该路径没有把 VID/PID、device_id、容量或 USB 序列号混入 onlyid 生成。
- 这与真实样本“相同硬件参数存在不同 onlyid”一致。
- GUID 本身是注册实例随机量；onlyid 只是其 32 位 CRC 压缩结果，不能从 onlyid 唯一恢复原 GUID。
- 跨平台制盘不需要依赖 Windows `CoCreateGuid` API，只需要 16B 高质量随机熵并复用相同 CRC32 算法。

#### EDPF 14B 表尾：完整字段名已恢复，不是终止符

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
    **正式命名但在已覆盖实现中未启用的兼容字节**；当前写入端=0，
    多代 Windows/Linux 读取端只结构性保留，不做值相关业务消费。
- 已闭合：
  - `bResetFileKey`：Windows `ChangePwd` 的强制改密分支会读取它；置位时重新生成 16B 文件密钥材料，否则保留并解包原文件密钥。
- 当前 22 份参考样本的表尾形态仍只有 4 种；`+0x0B..0x0D` 在该样本集均为 0，
  但这只是观察事实，不能解释成协议恒零。

#### 官方前部写集：固定 13 扇区

- `RegsiterUsb` 分配/读取缓冲长度为 `sector_size * 0x0d`，并调用：
  - `sub_100136c0(..., count=0x0d)`：从起点读取 13 扇区；
  - `sub_10013810(..., count=0x0d)`：写回 13 扇区。
- `sub_100136c0/sub_10013810` 内部都明确以 `count * sector_size` 计算读写长度，因此 `0x0d` 是数量。
- 从起点 LBA=0 开始，13 扇区正好覆盖 **LBA0..LBA12**。
- `u_disk` 的官方 DLL 虚拟注册 trace 同样记录 `start=0, count=13`。
- 当前产品契约因此统一为：备份、检查、生成、写入、读回、恢复全部只处理 LBA0–12，共 6656B。

#### EDPF：+0x08 是 PartionCount，不是版本

- LBA7（0x40 步长）和 LBA12（0x60 步长）均逐样本验证：`u32@entry0+0x08 == 实际连续 EDPF entry 数量`。
- 当前 22 份参考样本：LBA12 为 22/22 三条；LBA7 为 21 份三条、1 份两条。
- 在这 22 份“原始生成参考”内部，唯一 LBA7=2 的样本仍是 Netac `onlyid=949028302 @ 17:24:33`；与同 onlyid 的 17:23:49 / 17:24:20 对比，仅 LBA7 发生变化，其余 LBA0–12 一致，因此该扇区继续按局部实验/中间态降权。排除它后，原始 LBA7 参考是 21/21 三条。
- 新纳入的独立真实免密 SanDisk Ultra 则给出**第二个、且是真实在用的两条条目配置类型**：条目0=type2、条目1=type4，二者 `NeedDisturb=1`、`NeedEncrypt=1`，`PartionCount=2`。这证明“两条 LBA7”本身不能再被描述成只可能是实验态；它只说明 Netac 那一份不能用于反推原始三分区写入端。
- 对该真实免密盘重新按紧凑布局 0x40 ABI 逐字段解码时，两个条目的 `Version@+0x04` 都是 **0**；目录中旧 `disk4_info.json` 的 `"ver": 2` 来自历史解析器把 `PartionCount@+0x08` 错当成版本，现已由回归门禁明确拦截。
- 因此 `+0x08` 必须命名为 `partition_count` / `PartionCount`；表格式代际不能再从该字段推断。

#### LBA7 打包 64 字节 ABI 与 Linux 自然对齐 72 字节 ABI

本轮重新从 Linux DWARF、Windows 转换器和22份原始真实设备三条线核对
LBA7，确认同名 `tagEdpPartionInfo` 存在不能混用的 ABI：

- `libcemsfilesyscheck.so` DWARF：`sizeof(tagEdpPartionInfo)=0x48`，
  `UserKeyCRC@+0x30`、`FileKeyCRC@+0x38`、`EncryptFileKey@+0x40`；
  `BuildSector7@0x1DCDA` 复制 `0xD8=3*0x48`，自然对齐密码信息在 `+0xD8`；
- Windows 物理/运行时旧版表：步长固定 `0x40`，去掉自然对齐 ABI 中
  `+0x34..+0x37` 的对齐洞，因此物理布局为
  `UserKeyCRC@+0x30 / FileKeyCRC@+0x34 / wrapped8@+0x38`，表尾在 `+0xC0`；
- `cemsusbregsiter.dll::sub_10016490` 是官方旧版->新版转换器：逐字段把
  `3*0x40` 旧版条目扩为 `3*0x60` 运行时条目；目标先清零，所以
  运行时 `EncryptMode@+0x58` 为0；
- `edpediskctrl.dll::sub_100125B0` 是反向新版->旧版转换器，明确把运行时
  `+0x34/+0x38/+0x3C` 写回紧凑布局旧版 `+0x34/+0x38/+0x3C`。

22份原盘按设备 ID CRC 派生的 LBA7 滚动密钥重新解密：

- 紧凑布局 `0x40`：21/22 为三条 EDPF，1份已知局部中间态为两条；
  `+0xC0` 密码信息 22/22 都恢复合法版本（21份0x0064、1份0x0206）；
- 自然对齐 `0x48`：22/22 都无法得到三条连续 EDPF，`+0xD8` 密码信息
  也 0/22 合法；
- 因此 Linux 72B 自然对齐 ABI 只可用于字段名/源码来源参考，不能直接作为
  Windows 实盘 LBA7 物理偏移。

独立真实免密 SanDisk Ultra 也按同一设备 ID CRC/滚动异或重新解密，
得到 2×0x40 连续 EDPF、合法 `+0xC0` 密码信息（版本=0x0064），并且
两个条目的 `Version@+0x04=0`。仓库新增
`tests/fixtures/protocol_evidence/sandisk_ultra_authentic_no_password_lba7.hex`
与定向测试固定这一事实。该样本增加的是“真实配置类型行为”覆盖，不改变
22份原始生成参考集的计数，也不单凭样本值把版本/NeedDisturb 升级为完全闭环。

#### LBA7 条目版本 / 条目1+条目2 NeedDisturb：兼容元数据生命周期闭合

本轮直接回到官方机器码，而不是沿用旧结构猜测。

\`cemsusbregsiter.dll::sub_10016490\` 的旧版->新版转换循环对三条条目逐条执行
\`0x40 -> 0x60\` 映射，并明确复制 \`old+0x04 -> new+0x04\`（版本）和
\`old+0x10 -> new+0x10\`（NeedDisturb）。\`edpediskctrl.dll::sub_100125B0\`
的反向 \`0x60 -> 0x40\` 转换同样逐条复制这两个 DWORD，因此二者都是实际 ABI
字段，而不是反编译器误识别的洞。

对当前官方写入端 \`CUsbRegsiter::CreatePartitions/sub_1003DB50\` 再看机器码：

- 开头先 \`memset(old_table, 0, 0xC0)\`，一次清零完整的 \`3*0x40\` 旧版表；
- 随后显式写标志/PartionCount/PartionType/NeedDisturb/NeedEncrypt/几何/CRC/密钥；
- 三条条目都没有任何 \`+0x04\` 覆盖写，所以当前写入端的
  \`Version@+0x04=0\` 来自整表零初始化；
- 注册调用者对 CreatePartitions 的 NeedDisturb 参数固定传 \`1\`；
- 条目0、条目1 都显式执行 \`NeedDisturb=1\`；
- 条目2 没有对应覆盖写，因此继承整表清零值 \`0\`。

这解释了当前三分区配置类型的 \`1/1/0\`，但也证明它不是按 PartionType
定义的恒等规则。新纳入的真实免密 SanDisk 是两条条目：
type2/条目0 NeedDisturb=1，type4/条目1 NeedDisturb=1；而标准三分区原盘的
type4/条目2 NeedDisturb=0。仓库新增
\`lba7_need_disturb_is_not_a_partition_type_invariant\` 门禁，禁止以后把
\`type4 -> 0\` 写死。

消费端侧要区分“行为消费端”与“兼容结构性消费端”：

- 两版 Windows \`vrvaud_c\` 的 \`ReadPartionInfoExNew\` 都会把完整 \`0xC0\`
  紧凑布局旧版表读入运行时缓冲；
- \`NewCheckDisTurbUsb\` 与 \`NewCheckDisTurbUsbEx\` 只对
  条目0 \`NeedDisturb@+0x10\` 做非零门控；
- 本轮再次从两版全局缓冲的真实地址复核步长：ydcc
  `0x1020BF40` 与 Win10 `0x10172520` 都只分配/清零 `0xC0`，而各自
  `sub_*C5D0` 用 `(i << 6)+base+0x0C` 枚举三条 PartionType，证明这里是
  **3×0x40 紧凑布局旧版表**；此前把该全局区描述成0x60 运行时表的口径撤销；
- 两版对完整表地址区间的静态交叉引用结果一致：只有
  条目0 `NeedDisturb@base+0x10` 存在行为读取；条目1/条目2 NeedDisturb 与
  三条 `Version@+0x04` 都没有直接交叉引用，动态遍历也只读 PartionType；
- Linux \`CLabelManage::GetPartionFromOld\` 也只是把版本/NeedDisturb
  从旧版 ABI 搬到新版 ABI；
- Linux \`CLabelManage::GetPartionFromOld\` 对版本/NeedDisturb 只做字段搬运；
  继续逐函数复核 \`CDiskReader::GetTagPartitionInfo\`、\`DecryptFileKey\`、
  \`CheckFileKeyCrc\`、\`ReadFileSysSector0\` 与
  \`DecryptFileSysSector0\` 后，真实行为读取集中在标志/PartionType/
  UserKeyCRC、StartSector、FileKeyCRC、封装密钥与 EncryptMode；
  条目 \`Version@+0x04\` / \`NeedDisturb@+0x10\` 均未参与这些检查链；

旧版 \`EdpEDiskCtrl.dll\` 也再次表明：旧 LBA7 被读出后，协议代际由
14B 密码信息版本决定/被上层固定为 \`0x64\`，并未发现条目
\`Version@+0x04\` 用作版本选择。

实盘复核：已提交原始测试夹具的全部有效 EDPF 条目与新增真实免密
SanDisk 两条条目的 \`Version@+0x04\` 全部为0；扩展只读历史去重扫描同样
没有非零版本。NeedDisturb 则稳定出现三条目 `(1,1,0)` 与两条目 `(1,1)`
两种按位置配置类型，没有第三种组合。

因此这里不把负交叉引用误写成“保留”，而是按已经用于其它兼容槽的严格模型闭合：

- 3×版本共12B：当前写入端全零初始化；Windows 旧版/新版转换器双向保存；
  Windows 两版与 Linux 文件系统检查链都不把它作为版本选择条件；真正协议代际由
  14B 密码信息版本决定；
- 条目1/条目2 NeedDisturb 共8B：当前写入端分别形成 1/0 按位置配置类型；
  转换器保存；Windows 两版只有条目0 同名字段存在行为读取，Linux 对应检查链
  不读取条目1/2。它们不能继承条目0 的 MBR 扰动业务解释。

仓库门禁现对所有已提交原始测试夹具逐条目锁定版本=0 与
按位置 NeedDisturb 配置类型，并由独立 SanDisk 的 type4@条目1=1 继续阻止
`PartionType -> NeedDisturb` 的错误恒等化。

所以这20B由部分闭环升完全闭环，准确语义是
**正式 ABI 兼容元数据：写入端/配置类型已知、转换器结构性保留、
跨平台负向语义消费端、真实配置类型已锁定**。完全闭环不要求
未来其它写入端配置类型必须仍写0/1/0；遇到新配置类型应扩展兼容模型。

#### LBA7 v0x0064 打包旧版文件密钥封装

旧表 `+0x38..+0x3F` 8B 封装密钥本轮完成写入端/消费端闭合。

直接消费端是 `edpediskctrl.dll::sub_10026050`：

1. v0x0064 固定 `key_len=8`；v0x0206 才切到16B；
2. 从运行时条目 `+0x38` 复制封装材料；
3. 调 `sub_10028AB0(password, ..., EncryptMode, ...)` 解包；
4. 对解出的8B 调 `sub_10038840`（CRC32_bare）；
5. 必须等于条目 `FileKeyCRC@+0x34`，否则拒绝。

旧版转换器产生的 `EncryptMode=0` 进入专门旧算法：

```text
K = fold32(password)
fold32: little-endian 4B chunk 求和，尾部不足4B补零，u32 wrapping
plain_lo = wrapped_lo XOR K
plain_hi = wrapped_hi XOR K
```

机器码辅助函数已独立拆清：`sub_1005CEF0` 是无符号 64-位 shift 辅助函数，
`sub_1004EF80` 是无符号 64 位乘法辅助函数；化简后
`sub_10011450` 就是上述两个半字的 XOR 变换。逆向写入端
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

22份原始真实设备全量正向复算：

- 共28条非零 type2/type4 旧版条目；
- 28/28 的 `UserKeyCRC=0x0429735D`；
- 用 `K=0x91919191` 对 wrapped8 两个 DWORD 分别 XOR；
- **28/28** 解出的8B 文件密钥都满足
  `CRC32_bare(file_key8) == entry.FileKeyCRC`。

因此三条条目的 wrapped8 共 **24B 部分闭环 -> 完全闭环**。
`FileKeyCRC(+0x34..+0x37)` 此前已因 CRC 消费端链计入完全闭环，
本轮不重复把这4B/条目计数。新增回归门禁：

- `lba7_physical_entries_are_packed_64_not_linux_natural_72`；
- `lba7_v64_packed_legacy_file_key_wrap_matches_real_fixtures`。

#### LBA7 `+0x0CE..+0x1FF`：306B 写入端负责的全零表后区完整闭合

Windows `edpediskctrl.dll::sub_10010FC0` 是紧凑布局 LBA7 的直接写回写入端。
本轮继续回实际实现核对而不是沿用旧分析：

- `var_1020=0` 后调用 `sub_1004D110(&var_101F, 0, 0xFFF)`；
- `sub_1004D110` 的机器码已确认是 memset 等价实现（对齐后 `rep stosd`）；
- 随后只 `memcpy` 0xC0 紧凑布局旧版表到暂存 `+0x000`；
- 再复制0x0E 密码信息到 `+0x0C0`；
- 最后对完整256个16 位字，即512B做滚动 XOR并 `WriteFile` 到 LBA7。

因此 `+0x0CE..+0x1FF` 的306B不是“没人知道的尾巴”，而是显式全零初始化后
从未被任何结构写覆盖的写入端负责区。

消费端也闭合：Windows `ReadPartionInfoExEx/sub_10010B40` 会解码完整512B，
但魔数成功后只复制0xC0 表和0x0E 密码信息给调用者；`+0x0CE..+0x1FF`
完全不返回、不解析。Linux `BuildSector7@0x1DCDA` 独立体现相同的
“整块清零→结构写入→整扇滚动”原则，不过其自然对齐 ABI 表尾在0xE6，
所以只作为原则交叉，不拿 Linux 偏移覆盖 Windows 紧凑布局物理边界。

严格22份原始生成参考重新逐盘解密：**22/22 的 `+0x0CE..+0x1FF`
全部为全零[306]**，独立 SanDisk 原始也无反例。新增 CI 门禁：

- `lba7_post_table_plaintext_is_zero_through_sector_end`。

这306B因此满足区域边界 + 官方写入端 + 负向消费端 + 22盘原始证据，
可从未知 **直接升级完全闭环**。复核总账时同时纠正此前漏记的
条目0 `NeedDisturb` 4B：该字段早已由 Windows 写入端、两版
`NewCheckDisTurbUsb(*)` 有效消费端与22/22原盘闭合为完全闭环，但旧总数
没有加上这4B。因此 LBA7 严格状态应由实际的
`183 COMPLETE / 23 PARTIAL / 306 UNKNOWN` 先更新为
`489 COMPLETE / 23 PARTIAL / 0 UNKNOWN`；后续又闭合
`bNoUsbChkPasSafe(+0x0A)` 1B；本轮又闭合3条条目版本（12B）与
条目1/条目2 NeedDisturb（8B），最后将密码信息 `+0x0C/+0x0D`
两个 BackupPromptPeriod 字节按未启用兼容字段生命周期闭合。
因此 LBA7 当前为 **512 完全闭环 / 0 部分闭环 / 0 未知**。

#### LBA12：主运行时盘面是 96B 紧凑布局条目；不要与 104B 检查结构混用

对 LBA12 的“已知”采用更严格标准：**只知道偏移、长度或结构名，不等于知道字段语义**。
只有写端来源、读端消费、取值语义至少两项闭合，才计为“已知”；否则一律降为“部分已知”。

主运行时盘面 96B 步长已由两套独立代码闭合：

- Windows `cemsusbregsiter.dll::CreatePartitions` 构造 3 个 `0x60` 条目，并把
  `0x120 = 3 * 0x60` 字节写入 LBA12 条目区；
- Linux 挂载库 `libedpedisk.so::EdpDiskLayoutTagePartV2::LayoutParsedata`
  从物理扇区复制 **0x120B**，随后从物理 `+0x120` 读取 14B 表尾，并验证版本 0x206；
- `libedpedisk.so::Volume::GetPartitionHeader` 只按 96B 条目复制 `+0x00..+0x5f`，
  并直接读取 `entry+0x58` 的低 1B 做算法分派。

`libcemsfilesyscheck.so` 的 DWARF 还存在一个 **104B** `tagNewEdpPartionInfo`
（0x68），其 `BuildSector12(version=0x206)` 会处理 312B = 104×3，并使用
`+0x138` 表尾。这是文件系统检查/修复组件的扩展结构，不得反向覆盖主盘面的
96B 紧凑布局布局。

#### LBA12 96B 紧凑布局条目：当前字段置信度

| 偏移 | 长度 | 当前名称 | 当前置信度 | 证据/限制 |
|---|---:|---|---|---|
| +0x00 | 4 | 标志 = `EDPF` | 已知 | 写端固定写入；读端判魔数 |
| +0x04 | 4 | 版本/条目内兼容元数据 | **完全闭环** | 当前 3×96B 写入端整表零初始化后从不覆盖该DWORD；Windows/Linux 紧凑布局运行时只结构携带，协议版本由14B 密码信息版本决定；22份×3 条目与全树历史复算均为0 |
| +0x08 | 4 | PartionCount | 已知 | 写端来源 + 当前22/22均等于实际连续条目数 + Linux字段名 |
| +0x0C | 4 | PartionType | 已知 | 1=启动 / 2=共享 / 4=加密；Windows/Linux运行时均消费 |
| +0x10 | 4 | NeedDisturb | **完全闭环（条目0行为 + 条目1/2 兼容）** | 条目0 已由旧版 `NewCheckDisTurbUsb(*)` 回退行为门控闭合；条目1/条目2 当前按位置配置类型固定为1/0，Windows/Linux 运行时只结构保留而无值相关消费；22份历史均 `(1,1,0)` |
| +0x14 | 4 | NeedEncrypt | 已知 | Windows InitDiskInfo/UserLogin 实际消费；0=未加密，1=启用透明加密 |
| +0x18 | 8 | StartSector | 已知 | 写端计算、挂载端使用 |
| +0x20 | 8 | SectorSize | 已知 | 实盘=512；布局/挂载使用 |
| +0x28 | 8 | PartionSize | 已知 | 写端计算、UserLogin/挂载参数实际消费 |
| +0x30 | 4 | UserKeyCRC | 已知 | 密码校验链消费；默认密码CRC已复算 |
| +0x34 | 4 | FileKeyCRC | 已知 | 解封装密钥后 CRC 校验；Windows UserLogin 明确比较 |
| +0x38 | 16 | 封装文件密钥材料 | **完全闭环** | 模式1/2/3 的 UI→请求→写入端可达链、官方写入端/消费端算法均已闭合；22份原始实盘44条加密条目全部为模式2。新增隔离 Unicorn 虚拟盘证据直接执行官方 `CEMSUsbRegsiter.dll::CreatePartitions`：模式1/2/3 最终条目0 封装调用点分别命中 `0x1003ED77/0x1003ED44/0x1003EDA7` 各1次，三份512B LBA12 盘面镜像由官方二进制原生生成；以 `ProofPass1!` 为输入，A6B0、标准SM4-ECB、标准AES-128-ECB 三套独立读取端均恢复同一16B 文件密钥 `147196f5a2ec7912edf13f75d766cb42`，CRC均=`0xFF4C1D36`并等于条目.FileKeyCRC。虚拟写入端测试夹具明确不计入物理真实设备统计集，但它不是edpcli公式合成数据，而是一方可执行文件运行时正向盘面证据 |
| +0x48 | 16 | `EncryptFileKey32[16]` 跨代兼容槽 | **完全闭环** | 旧版 72-字节 ABI 不存在 EncryptFileKey32 槽；104B 检查器 ABI正式命名自然对齐 `+0x50 EncryptFileKey32[16]`，但旧版→新版转换器不填，DecryptFileKey/CRC/文件系统解密均不读；紧凑布局运行时只结构缓存该16B而无值相关读取，当前 Windows 写入端显式清零；22盘66/66 条目全零 |
| +0x58 | 1 | EncryptMode | 已知 | Windows 写入端/读取端 + Linux挂载分派；见下方支持矩阵 |
| +0x59 | 7 | 保留[7] | **完全闭环** | 官方 DWARF 明确命名保留[7]；Windows CreatePartitions 对 3×96B 整表先清零且只写到 +0x58；Windows/Linux 运行时登录/改密不消费该区；22盘 66/66 条目为零 |

`EncryptMode` 的枚举由 Linux DWARF 直接给出：

- 0 = `eEncryptAES64`
- 1 = `eEncryptAES128`
- 2 = `eEncryptSMS4`
- 3 = `eEncryptAESOPENSSL`

但**枚举存在不等于每个组件都实际支持**：

- `libedpedisk.so::GetPartitionHeader` 主挂载路径：0→OldEdp，1→AES128，2→SMS4；
  其它值在该构建不创建可用头；
- `libcemsfilesyscheck.so::fileKey_Decrypt` 当前构建只实现模式=1/2；
- Windows 写入端有 1/2/3 的封装密钥写入分支；
- Windows `UserLogin` 对模式=3 有“先按3解，CRC失败后按1重试”的兼容路径。

因此不能把“0/1/2/3”简单写成统一跨版本算法支持表。

`NeedEncrypt` 已由 Windows 运行时代码闭合：

- `InitDiskInfo` 在共享条目上读取 `entry+0x14` 并保存到运行时状态；
- `UserLogin` 在共享存在但该状态为 0 时直接记录
  `There are unencrypted!`；
- 所以该字段可定性为：0=该分区不启用透明加密，1=启用透明加密。

`NeedDisturb` 已补到“字段名 + 写端来源 + 正向兼容消费路径”，但这个结论有明确版本边界：

- Windows 写入端直接写入 `CreatePartitions(arg2)`；
- 对已逆向的标准三分区创建分支，写端结果已经按 96B 条目基址重新核对：
  - 启动 = 1；
  - 共享 = 1；
  - 加密 = 0；
- 排除 edpcli 自生成的 `_nopwd_` 备份后，当前真实参考样本全部与该写入端配置类型一致：
  type1=1、type2=1、type4=0；
- 这说明“真实参考集 + 当前 Windows 写入端”目前没有冲突，但仍不能把它升级成
  `PartionType -> NeedDisturb` 的协议恒等式；
- Windows `UserLogin` 真实机器码与 `EdpMountFile` 参数结构已经对齐：
  `NeedEncrypt`、StartSector、PartionSize、FileKey、FileKeyCRC、EncryptMode 会进入挂载参数，
  但 `NeedDisturb` 没有进入当前用户态→挂载库→驱动参数链；
- Windows 主 DLL 中 `entry+0x10` 的其它命中均是 96B 条目之间的结构复制；
  与之相对，`UserLogin` 对同一条目明确读取 `+0x0c/+0x14/+0x28/+0x30/+0x34/+0x38/+0x58`，
  未出现对 `+0x10` 的条件判断或参数映射；
- Linux `libedpedisk.so::EdpDiskLayoutTagePartV2::LayoutParsedata` 只把 3×96B 条目
  整块复制进内存；`Volume::GetPartitionHeader` 再按值复制整条 96B 条目；
  `PartitionHeader` 构造函数把 `NeedDisturb/NeedEncrypt` 这一 8B 保存到对象
  `+0x50..+0x57`。继续扫描 `PartitionHeader*` 方法后，未找到构造之后对对象
  `+0x50/+0x54` 的业务读取或分支；
- 旧版 `vrvaud_c.m::ReadPartionInfoExNew` 会将解出的紧凑布局 EDPF 表
  复制到全局 `0x1020BF40`；同文件 `sub_1003c5d0` 用
  `base+0x0C+i*0x60` 读取类型 1/2/4，证明该全局区确实是 96B 步长表；
- 因此 `dword_1020BF50 = base+0x10` 精确落在 **条目0.NeedDisturb**。
  `NewCheckDisTurbUsb` 和 `NewCheckDisTurbUsbEx` 在
  `ReadPartionInfoExNew == EDP_SUCCESS` 后都执行：
  `if (entry0.+0x10 != 0) { out=1; success=1; }`；
- Windows 10 驱动包的另一套 `vrvaud_c.m` 独立出现同样布局：
  表基础版本=`0x10172520`、类型=`base+0x0C+i*0x60`、
  判断=`dword_10172530 = base+0x10`；
- 上层 `AllCheckModeUsb` 会把 `NewCheckDisTurbUsb(*)` 成功结果作为一条
  独立识别/处理分支继续执行，因此这不是仅复制后从不读取的死字段；
- 2026-06-29 Aigo U335 的真实客户端日志证明
  `NewCheckDisTurbUsb -> GetNewTagePartionInfo -> AllCheckModeUsb`
  是实际运行路径；但该次走的是新版 `GetNewTagePartionInfo` 成功分支，
  **不能**冒充回退 `+0x10` 判断的动态实测。

因此能闭合的行为只到：
**条目0 +0x10 是旧版/兼容 NewCheckDisTurbUsb 回退的成功门控位。**
这仍不足以把字段名翻译为“扰码开关”“防篡改”“激活”“只读”等更具体功能。
新版主路径和 Linux 主挂载链仍可能只保留/透传该字段。

对制盘的直接约束是：标准共享/条目0 的 `NeedDisturb`
必须保持非零；新增生成回归锁定 `entry0+0x10 == 1`。
这条消费者只读条目0，不能据此推导 type4 的固定取值。

#### LBA12 密码信息：密码状态组进一步闭合

14B 表尾当前能确认：

- `+0x00..01`：版本；
- `+0x03`：共享最大密码错误次数；
- `+0x04`：共享当前错误次数；
- `+0x06`：加密最大密码错误次数；
- `+0x07`：加密当前错误次数。

Windows `ChangePwd/sub_10026050` 进一步证明：

- 共享改密成功时同时清 `tail+0x02` 与 `tail+0x04`；
- 加密改密成功时同时清 `tail+0x05` 与 `tail+0x07`；
- Windows 登录入口在真正执行密码校验之前调用独立检查函数：
  type2 读取 `tail+0x02`，type4 读取 `tail+0x05`；对应字节非零时直接阻断普通登录。
  结合“改密成功清零”，`+0x02/+0x05` 的行为可闭合为共享/加密
  **强制改密状态标志**，不再只是“密码状态组成员”；
- `tail+0x0B` 也已出现真实消费者：仅在版本>=0x64、目标分区的强制改密标志非零、
  且 `tail+0x0B` 非零时，`ChangePwd` 才调用 `sub_10029e20` 生成新的 16B 材料；
  该生成函数以 `CoCreateGuid` 为源形成 16B 输出。随后代码重新计算 FileKeyCRC，
  再把该 16B 材料按新密码重新包装。若条件不成立，则走“解开原封装密钥并校验 CRC”
  的保留旧文件密钥路径。因此 `bResetFileKey` 可闭合为：
  **强制改密时是否同时重新生成文件密钥材料的门控**；
- `+0x0A bNoUsbChkPasSafe` 在当前 Windows 主 DLL 中被复制到对外结构的一个独立字节。
  机器码已确认该复制发生在
  `CEdpEDiskCtrlInterface::Init`：`m_PassInfo+0x0A -> Init输出+0x11`；
  `EdpEDisk.exe` 在初始化时把应用对象 `+0xA4` 作为该输出结构传入，因此该状态会被
  暴露到应用层；此前在 Windows 应用本体没有找到对对应 `app+0xB5` 的直接读取；
- 本轮把 `+0x0A` 写入端再向上追了一层：当前
  `CUsbRegsiter::CreatePartitions/sub_1003DB50` 先把完整14B 密码信息
  `memset(..., 0, 0x0E)`，随后机器码
  `1003E78A..1003E790` 明确执行
  `tail+0x0A = create_arg1+0x109`；而
  `WriteNormalULabel -> sub_10046E80` 又明确执行
  `create_arg1+0x109 = UsbWriteParam+0x7EC`。因此 `+0x0A`
  是注册/制标请求中的显式1B配置输入，不是未初始化噪声或尾部填充；
- 同一个当前 CreatePartitions 首次建表路径对 `tail+0x0C/+0x0D`
  没有任何覆盖写；二者直接继承 14B 全零初始化。**这一阶段**结合22份原始参考与
  新增真实免密 SanDisk 都为0，只能确认当前写入端的零来源，尚不足以升级；
  后续已补齐四代 Windows 读取端、Linux 结构性保留、独立策略排除和
  扩展历史配置类型证据，最终按未启用兼容字段生命周期完全闭环，
  见本节后续更新；
- Linux `CDiskReader::ParseSector12` 把完整 14B 密码信息保存到
  `CDiskReader+0x210`。机器码全模块扫描可找到 `Version @+0x210`
  在 `DecryptFileKey` 中的显式读取，却没有找到
  `+0x21A/+0x21C/+0x21D`（分别对应密码信息
  `+0x0A/+0x0C/+0x0D`）的直接业务读取。这只是
  **libcemsfilesyscheck.so 单模块**的负证据；其中 +0x0A 后续已在独立
  `checkdiskback` 找到真实消费端，不能再写成“全产品不消费”；
- 当前 22 份原始参考样本中，`bNoUsbChkPasSafe(+0x0A)` 并非恒零：
  **18/22=0、4/22=1**；且每一份样本的 LBA7/LBA12 取值都逐字节一致。
  因此它明确是会随标签状态变化并跨两份表同步保存的真实字段，绝不能归为填充；
- 本轮从此前未纳入主审计的独立官方 Linux 可执行文件
  `checkdiskback` 找到该字段的真实行为消费端：
  `Update_EDPEDISKSHOWPARAM(checkdisk::_EDPEDISKSHOWPARAM*,
  tagEdpPartionPassInfo*) @ 0x406B70` 的首条逻辑就是
  `cmp byte [pass+0x0A],1; setne [showparam+0x03]`。
  这不是 memcpy/不透明往返验证，而是根据字段值生成布尔策略位：
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
  显示参数尾组写入运行时对象，其中 showparam+3 落到
  `runtime+0x20041`；相邻日志明确打印显示共享/显示加密/自动登录/
  强制改密等策略字段；
- 因而 `bNoUsbChkPasSafe` 已满足本项目严格完全闭环标准：
  **显式制标写入端 + 值相关行为消费端 + 加密策略跨组件传递 +
  22份原始实盘0/1双值与 LBA7/LBA12 同步证据**。LBA7 `0x0CA` 与
  LBA12 `0x12A` 各1B均为完全闭环；
- 2026-09-24 又从官方制盘 UI 向下补齐了生产端语义链。`WriteLabel`
  构造函数 `sub_4650a0` 以 `this+0x14` 作为 `Ui_writeLabel` 基址调用
  `setupUi/sub_479910`；其中 `Ui+0xE4` 的对象名明确为
  `pwdComplexityCheckBox`，所以运行时对象地址是 `WriteLabel.this+0xF8`。
  `retranslateUi/sub_487660` 对该控件调用 `QAbstractButton::setText` 时，
  PE 中源字符串地址 `VA 0x4B5AD4` 解出精确中文 **“取消密码复杂性验证”**；
- `sub_466eb0` 的原始机器码再从 `WriteLabel.this+0xF8` 调用
  `QtGui4!QAbstractButton::isChecked()`，返回值不做任何取反，直接保存到
  前端请求结构 `+0x48`；结构转换器 `sub_42e8e0` 又逐字节执行
  `request+0x48 -> LabelInfo+0x7EC`。同一官方 DLL 的
  `UsbtoolBusMgrInter::LabelInfo::Print` 把 `+0x7EC` 明确打印为
  `complexity`。结合已经闭合的
  `UsbWriteParam+0x7EC -> CreatePartitions request+0x109 -> PassInfo+0x0A`
  写链，可以确定业务语义方向：
  **0 = 未勾选“取消密码复杂性验证”；1 = 勾选，即取消/跳过密码复杂性验证**。
  因此人类可读名称应更新为“取消密码复杂性验证”，同时保留正式 ABI 原名
  `bNoUsbChkPasSafe` 以便和二进制/DWARF 对照；
- `+0x0C/+0x0D` 当前 Windows 主 DLL、另一版 `out_raw_data/EdpEDiskCtrl.dll`、
  Linux `libcemsfilesyscheck.so`，以及本轮补扫的 Linux
  `EdpEDiskQt5/EdpEDiskBack/linuxedpedisk` 客户端路径均未找到直接消费者；
  Linux DWARF 只给出
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 官方字段名。
  当前 22/22 原始参考样本的两字节均为 0；
- 与 `+0x0B bResetFileKey` 类似，“真实样本全零”本身不能推出填充。
  本段是较早阶段结论；后续通过写入端 + 多代结构性保留 +
  跨平台负向语义消费端证明它们是**正式但未启用的
  兼容字段**，不再要求虚构一个当前实现中不存在的有效周期消费端；
- 上述“不得把 `bNoUsbChkPasSafe` 翻译成确定业务行为”的旧限制已被
  2026-09-24 的官方 UI 生产端证据解除：该字节就是
  `pwdComplexityCheckBox` 的“取消密码复杂性验证”勾选状态，且 UI 到
  `PassInfo+0x0A` 的已确认链路没有取反。后续文档与 inspect 均应使用这一
  明确语义，不再称作泛化的“免密安全策略”。

本轮又补做了**跨版本 Windows 消费端审计**，结果进一步收紧而没有升完全闭环：

- 当前 `edpediskctrl.dll` 的密码信息运行时基址可由已闭合字段反推出：
  `word_10092A20=Version(+0x00)`、`byte_10092A28=+0x08`、
  `byte_10092A29=+0x09`，因此剩余三字节精确映射为
  `0x10092A2A(+0x0A)`、`0x10092A2C(+0x0C)`、`0x10092A2D(+0x0D)`；
- `+0x0A` 在当前 DLL 中只有一个直接交叉引用：
  `Init输出+0x11 = pass_info+0x0A`。解析接口对象虚表后确认，
  该代码属于 `CEdpEDiskCtrlInterface::Init`（虚表 slot1），不是独立策略函数；
- 当前 `edpedisk.exe` 在 `CEdpSecDiskAppApp::InitInstance` 中以
  `app+0xA4` 作为初始化输出，因此 `+0x0A` 最终落在 `app+0xB5`；
  全文件扫描没有任何 `app+0xB5` 读取，`app+0xA4` 也只在这次初始化调用中出现；
- 旧版 `/VRV/edp/EdpEDiskCtrl.dll` 可独立反推出
  `pass_info base=0x10063398`：`+0x08=0x100633A0`、
  `+0x09=A1`、`+0x0A=A2`、`+0x0C/+0x0D=word_100633A4`。
  该版本同样只把 `+0x0A` 复制到初始化输出 `+0x11`，没有策略分支；
  `+0x0C/+0x0D` 只见成对清零，没有读取交叉引用；
- 旧 `edpedisk.exe` 同样只把 `app+0xA4` 传给初始化，后续没有消费
  `app+0xB5`。因此“仅向外暴露、当前宿主未消费”的边界至少跨两代 Windows
  实现成立，不是单个构建偶然遗漏；
- `vrvaud_c` 中的 `BackupPromptInfo/BackupStartTime/BackupEndTime`
  已追到备份 UI 与时间窗口逻辑，但没有任何数据流连接到密码信息
  `+0x0C/+0x0D`，禁止仅凭名称相近把两者合并解释。

严格22份再次独立复算：

- `bNoUsbChkPasSafe(+0x0A)`：18份为0、4份为1；
- 4份非零均是真实原始参考，且均为 LBA7 版本 0x0064；
- 22/22 的 `+0x0A` 在 LBA7/LBA12 两份副本中一致；
- `+0x0C/+0x0D` 仍是22/22全零。

这里的跨版本 Windows 审计最初只用于继续收紧 `+0x0C/+0x0D` 的边界；
`+0x0A` 已在后续独立 `checkdiskback::Update_EDPEDISKSHOWPARAM` + Safe6PolicyFile
链中找到值相关消费端，并已按主账本升级完全闭环，不能再沿用本段较早阶段的
“缺最终策略消费端”结论。对 `+0x0C/+0x0D` 的后续结论也已更新：它们不是
等待某个必然存在的有效消费端，而是 **未启用兼容字节**。

新增证据如下：

- Linux DWARF 直接把 `edpdiskglobal.h:164/165` 的两个成员定义为独立
  `BYTE`：`ShareBackuppromptPeriod@+0x0C` /
  `EncryptBackuppromptPeriod@+0x0D`，排除填充/位域；
- 除当前 ydcc 与 out_raw 两版外，又补审
  `VRV/cems/Edp/edpediskctrl.dll` 与
  `VRV/cems/Edp/edpdrivers_win10/EdpEDiskCtrl.dll` 两个不同哈希旧构建。
  四代读取端的成功路径都把完整14B 密码信息结构性复制到输出；
  Win10 旧构建可直接看到3×DWORD + 最后1×字的完整14B搬运，其中最后
  字就是 `+0x0C/+0x0D`。随后字段处理仍只命中
  `Version(+0)`、共享区重试 `(+3)`、保密区重试 `(+6)`；
- Linux 检查器保存完整14B结构，但实际解密/文件系统检查路径不读取这两个字节；
- 对两代 `vrvaud_c` 的 `BackupPromptInfo/BackupStartTime/BackupEndTime`
  继续追到机器码，确认它们从策略字符串解析到独立字符串/DWORD 全局变量，
  与14B 密码信息没有写回/映射数据流；
- 全二进制树 ASCII + UTF-16 精确搜索表明，
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 官方名称只存在于
  `libcemsfilesyscheck.so` 的 DWARF 类型信息，没有隐藏的第二套实现；
- 除已提交原始样本逐盘 LBA7/LBA12 两副本0/0一致外，
  全目录去重扫描19个真实 LBA7 密文配置类型（同时覆盖密码信息
  `v0x0064` 与 `v0x0206`）仍19/19=0/0。

因此严格完全闭环的语义不是“已经猜出周期的历史设计单位”，而是：
**这两个正式字段在所有已覆盖写入端/消费端代际中处于未启用状态；
当前写入端明确写0，读取端只结构保留/返回而无值相关语义消费。**
若未来出现非零旧配置类型，应原样保留/报告并新增配置类型，而不能按当前
实现机械清零或强行解释成小时/天。

对当前写入端的机器码边界又做了一次逐存储复核：
`CUsbRegsiter::CreatePartitions/sub_1003DB50` 在 `0x1003DC16..0x1003DC26`
对完整14B 密码信息执行显式清零；后续赋值序列最远只在
`0x1003E78A..0x1003E790` 写到 `pass+0x0A`，没有任何存储命中
`+0x0C/+0x0D`。因此这2B在当前 LBA7 v0x0064 与随后仅改版本为
v0x0206 的 LBA12 路径中都是**明确写入端负责全零**，不是“没有观察到赋值”或
未初始化保留底层字节。两代 `vrvaud_c` 的 `BackupPromptInfo` 虽有0/非0行为分支，
但其磁盘旧版表全局只承接 `0xC0 = 3×0x40` 紧凑布局条目，密码信息尾部
根本不在该全局内；当前也没有其它数据流把该策略项接到这2B。故名称相似不能
作为消费端证据。

Linux `PartitionHeader::SetPartitionNewPass` 同时给出负证据：

- 新密码只更新 `UserKeyCRC(+0x30)`；
- 新版 0x206 表只更新 `封装密钥(+0x38..+0x47)`；
- 不修改 `+0x48..+0x57`、`+0x58`、`+0x5c..+0x5f`。

因此这些区域不能解释成“密码修改状态缓存”。

#### LBA12 打包保留[7]：写入端 / 负消费端 / 真实设备闭环

96B 紧凑布局条目的 `+0x59..+0x5F` 本轮从部分闭环升为完全闭环，
不是因为“66/66 都是零”，而是四条证据同时闭合：

1. **官方字段定义**
   Linux DWARF 的 `tagNewEdpPartionInfo` 明确把 EncryptMode 后的 7B
   命名为 `Reserved[7]`。104B 自然对齐 ABI 中它位于
   `+0x61..+0x67`；96B 紧凑布局运行时去掉对齐洞后对应
   `+0x59..+0x5F`。

2. **写入端零来源**
   Windows `CUsbRegsiter::CreatePartitions / sub_1003DB50` 开头执行
   `memset(var_1364, 0, 0x120)`，一次清零完整的
   **3×96B 紧凑布局条目**。后续逐字段构造只写到
   `+0x58 EncryptMode`，没有对 `+0x59..+0x5F` 的覆盖。

3. **负向消费端**
   Windows `UserLogin` 只从条目 `+0x38` 复制16B 封装密钥，
   从 `+0x58` 读取 EncryptMode；改密码路径 `sub_10026050`
   同样只读写 `+0x38..+0x47` 并读取 `+0x58`。
   Linux `CDiskReader::DecryptFileKey` 读取 EncryptMode 后不读取
   `Reserved[7]`；`libedpedisk.so::SetPartitionNewPass` 对 v0x206
   也只更新16B 封装密钥。

4. **22盘实测**
   对 22份原始真实设备参考集解密后的全部
   **66条 EDPF 条目** 重算：`+0x59..+0x5F` **66/66 全零**，
   无任何非零反例。

这里还顺带纠正一个 ABI 易错点：

- `libedpedisk.so::PartitionHeader` 构造函数明确记录
  `header_size=0x60`，其按值参数是 **96B 紧凑布局 ABI**；
- `libcemsfilesyscheck.so` DWARF 的同名
  `tagNewEdpPartionInfo` 是 **104B 自然对齐 ABI**，
  `FileKeyCRC` 前存在4B对齐洞，并含 `EncryptFileKey32[16]`；
- 同名 C 结构在两个 Linux 组件中**不能按偏移直接混用**。

保留[7] 闭合后又继续独立追了相邻紧凑布局 `+0x48..+0x57`。最终不能把
`EncryptFileKey32[16]` 的正式名字本身当成“仍有隐藏算法”的证据：

- 旧 `tagEdpPartionInfo` 只有72B，`EncryptFileKey@+0x40` 后即结束；
  **旧版 72 字节 ABI 不存在 EncryptFileKey32 槽**；
- 自然对齐 104B `tagNewEdpPartionInfo` 才增加 `EncryptFileKey32[16]@+0x50`；
  `CLabelManage::GetPartionFromOld` 将旧72B 条目迁移到104B时只复制旧密钥到
  自然对齐 `+0x40`，从不填 `+0x50`；
- 检查器的 `CDiskReader::DecryptFileKey` 只取自然对齐 `+0x40..+0x4F` 主16B 密钥
  与 `+0x60 EncryptMode`，`CheckFileKeyCrc/ReadFileSysSector0/DecryptFileSysSector0`
  同样没有 `+0x50` 值相关读取；
- 紧凑布局 `libedpedisk.so::PartitionHeader` 构造器确实按值缓存完整96B，因此
  紧凑布局 `+0x48..+0x57` 会落到对象 `+0x88/+0x90`。但按全部
  `PartitionHeader` 符号边界重扫，两个QWORD只在构造函数写入，后续算法方法不读；
  正对照主 16B 封装密钥的对象 `+0x78/+0x80` 中，`+0x78` 被
  SMS4/AES128/OldEdp 解密实际取址并按16B消费；
- Windows `CreatePartitions` 对3×96B先 `memset(0,0x120)`，后续只写主 16B 封装密钥
  与模式；Windows UserLogin/改密与 Linux 紧凑布局挂载/update 也都不消费扩展槽；
- 严格22份原始参考的全部66条条目中，该16B **66/66全零**。

因此紧凑布局 `+0x48..+0x57` 的当前可观察行为已经闭合为正式命名但无当前算法消费的
**跨代兼容槽**：旧ABI不存在，新ABI保留并可随结构搬运，
当前紧凑布局写入端显式零，运行时仅结构缓存。三个条目共48B从部分闭环升
完全闭环。完全闭环不表示所有未来ABI都必须写零；若发现独立非零配置类型，应结构保留。

**LBA12 EncryptFileKey32 兼容槽结构缓存 / 负语义消费端闭环**
与保留[7] 门禁独立存在，禁止把两者重新合并解释成填充。

**LBA12 打包 `Reserved[7]` 写入端/负消费端闭环**
已加入回退门禁，防止以后再次把 `+0x48` 扩展槽与
`+0x59` 保留混成一片“全零填充”。

按上述严格口径，Windows/Linux 主运行时 96B 紧凑布局 LBA12 当前逐字节进度为：

- **完成 512B / 512B（100.0%）**
  - 三个条目中语义闭合字段：49B/条目，共 147B；
  - 条目0 `NeedDisturb(+0x10)`：4B，旧兼容消费端 + 22/22 原始盘已闭合；
  - 三个条目的 `Version(+0x04)`：12B，当前全零写入端 + 紧凑布局运行时结构性保留/负向语义消费端 + 22×3实盘闭合；
  - 条目1/条目2 `NeedDisturb(+0x10)`：8B，当前按位置 1/0 写入端 + 跨平台负向语义消费端 + 22盘实测闭合；
  - 三个条目的 `EncryptFileKey32[16]` 兼容槽：48B，当前零写入端 + 结构性缓存/负向语义消费端 + 66/66实测闭合；
  - 三个条目的 `Reserved[7]`：21B，写入端 + 负向消费端 + 66/66 条目实测闭合；
  - 三个条目的封装密钥 `+0x38..+0x47`：48B，模式2由44条真实设备条目闭合；模式1/模式3由官方 `CreatePartitions` 隔离动态执行 + 独立算法解包 + FileKeyCRC 往返验证闭合；
  - 表尾已闭合字段：14B（含 `bNoUsbChkPasSafe` 与两个未启用 BackupPromptPeriod 字节）；
  - `0x12e..0x1ff`：210B，写端零初始化且主读端不消费，可定性为表后全零填充；
- **部分已知 0B / 512B（0%）**
- **未知 0B / 512B（0%）**
  - 当前主运行时格式已经没有未闭合字节。

这组数字只描述**主运行时 96B 紧凑布局格式**；不把 `libcemsfilesyscheck.so`
的 104B 扩展结构混入统计。

#### LBA12 封装材料的当前拆分

LBA12 条目 `+0x30..+0x47`：

- `+0x30..+0x33`：`CRC32_bare(password)`；默认 `"0000aaaa" -> 0x0429735D`。
- `+0x34..+0x37`：`CRC32_bare(file_key)`。
- `+0x38..+0x47`：16B 封装文件密钥材料。
- 旧 23 份混合集曾得到的封装密钥统计已撤销；本节只使用
  **22 份原始真实设备参考集**。

本轮已经把 v0x0206 默认密码分支重新从官方写入端/消费端和 22盘
独立闭合，形成 **LBA12 v0x0206 隐藏的默认密码文件密钥封装**
证据链。

Windows 写入端（`CUsbRegsiter::CreatePartitions / sub_1003db50`）：

1. 在构造紧凑布局 96B 条目前生成一份 16B 文件密钥；
2. `entry+0x34 = CRC32_bare(file_key16)`；
3. `entry+0x30 = CRC32_bare(password_before_default_substitution)`；
4. 当密码恰为 `"0000aaaa"` 时，先调用
   `sub_10040400(password)` 把它替换为隐藏 10B 字符串，再执行后续
   MD5 + 文件密钥包装；
5. EncryptMode=2 且配置 `GLOBAL/oldSM4 != "1"` 时走
   `sub_100036e0 -> sub_100031a0/sub_10003550`，该实现逐常量/轮函数
   对应标准 SM4（FK/CK、S-box、32轮、密钥扩展 T' rot13/23、
   轮函数 T rot2/10/18/24）；
6. 16B 包装结果写 `entry+0x38..+0x47`。

`sub_10040400` 的隐藏字符串本轮没有沿用旧脚本，而是从当前 Windows DLL
机器码重新提取：

- 32B 种子：
  `468b46088b4e048bd02bd13bd37f2183c1098d3c003bf97f028bf98b065750e8`；
- 96B 表和 16B 索引全部逐字节从该函数栈初始化恢复；
- 前16B xor 后16B 得第一轮 A6B0 密钥：
  `8782cb348b75fdf4d2a028b0d528716b`；
- 前16B 封装值相加 后16B 得第二轮 A6B0 密钥：
  `0794d3448b89fd0ad2b6cac6d9d6716b`；
- 解出的索引前10B为：
  `38 17 51 4d 0c 05 26 16 2d 0d`；
- 最终从解码后表取值得到：
  **`LtSWi[2f)j`**；
- `MD5("LtSWi[2f)j") = 548b072cba7f104d88a446556cc3c432`。

Linux 消费端独立给出同一设计：

- `CDiskReader::DecryptFileKey @ 0xDF50` 先构造 `"0000aaaa"`；
- 密码信息版本=`0x0206` 且输入仍为默认密码时，调用
  `GetIniString(char*) @ 0xCCCC`；
- `GetIniString` 自身包含与 Windows 对应的种子/表/索引混淆逻辑，
  最终覆盖输入前10B；
- `AlgorithmSpace::fileKey_Decrypt @ 0xC7B4` 对模式2：
  `MD5(effective_password)` 后调用 `MC_KKSMS4::DecryptBuffer`；
- `CDiskReader::CheckFileKeyCrc @ 0xE170` 对解包出的 16B 文件密钥
  重新 `CRC32`，必须等于条目保存的 `FileKeyCRC`。

22份原始盘重新解 LBA12 后共有 66 条紧凑布局条目：

- 22 条启动/type1 或旧 type2 条目为 mode0、封装密钥全零；
- 44 条 type2/type4 为 **EncryptMode=2**；
- 其中 43 条 `UserKeyCRC=0x0429735D`，即盘面密码标识仍对应
  原始默认字符串 `"0000aaaa"`；
- 另 1 条（Netac onlyid=3274129259 的 type2）为非默认
  `UserKeyCRC=0x438C9FFC`。

为避免旧分析脚本自证，本轮又使用系统 OpenSSL 3.6.3 的
**标准 SM4-ECB** 独立解 43 条默认模式2 封装材料：

- **43/43** 成功恢复 16B 文件密钥；
- **43/43** 满足
  `CRC32_bare(unwrapped_file_key) == entry.FileKeyCRC`；
- 对唯一非默认 type2 的同一实盘，其 type4 仍使用默认密码；
  从 type4 独立恢复共享文件密钥
  `eadd58009f9abe0625a1f1f779d4c98b`，
  得到 `CRC32=0xF7EEA980`，与 type2/type4 两条条目的
  FileKeyCRC 均精确一致；
- type2/type4 的 16B 封装密钥不同，证明“同一文件密钥 +
  不同 effective 密码分别包装”的模型，而不是复制同一密文。

因此，对当前 22份原始盘实际使用的
`version=0x0206 + EncryptMode=2 + oldSM4!="1"` 配置类型，
`+0x38..+0x47` 的写入端、消费端、默认密码替换规则和实盘结果
已经完整闭合。

本轮随后继续把其它算法分支追完，形成
**LBA12 备用封装模式算法映射**。

##### 模式2 的 `oldSM4` 开关不是新的盘面格式

Windows 写入端的两个模式2 实现分别是：

- `oldSM4=="1" -> sub_10011010`；
- 其它 -> `sub_100036e0`。

对 `sub_10011010` 再回到机器码/常量逐项核验：

- S-box @ `0x100C6548` 与标准 SM4 256B S-box 完全一致；
- FK 在内存中以小端 DWORD 保存，解释后仍是
  `A3B1BAC6 / 56AA3350 / 677D9197 / B27022DC`；
- CK 同样以小端 DWORD 保存，解释后从
  `00070E15 / 1C232A31 / 383F464D ...` 开始，完整对应标准32轮 CK；
- `sub_1000FD90` 是32轮密钥 expansion；
- `sub_10010B80` 是标准轮函数 T；
- `sub_1000FFA0` 正向使用轮密钥；
- `sub_10010540` 反向使用同一轮密钥。

因此 `sub_10011010` 与 `sub_100036e0` 都实现
**SM4-ECB(16B, MD5(effective_password))**，只是内部实现不同。
更重要的是 Windows `CEdpDiskControl::UserLogin -> sub_10028AB0`
在模式2 只有一条标准 SM4 解包路径，而且完全不读取 `GLOBAL/oldSM4`。
所以该配置不能代表不同的盘面格式；否则同一读取端无法同时读取两种盘。
本账本将 `oldSM4` 定性为 **实现分支选择, 不是盘面配置类型分支选择**。

##### EncryptMode=1：EDP A7F0/A6B0 16B 封装

Windows 写入端：

```text
effective_password
  -> MD5 = 16B
  -> sub_10001190(file_key16, key=MD5)
  -> wrapped16
```

`sub_10001190` 的密钥初始化先执行：

```text
expanded_key_input[i] =
    MD5[i] XOR "EDPSECDISK200709"[i]
```

随后使用项目已独立恢复的 A7F0 正向块算法；16B 文件密钥只有一个块，
计数器从0开始。

Windows 消费端 `sub_10028AB0 case 1`：

- 对输入密码做 MD5；
- 调用 `sub_100384E0`；
- 其密钥初始化同样 XOR `"EDPSECDISK200709"`；
- 使用与写入端相反的 A6B0 解包；
- `UserLogin` 随后 CRC32 16B 明文并比较 `FileKeyCRC`。

Linux `AlgorithmSpace::fileKey_Decrypt case 1` 也执行
`MD5(password) -> Decrypt(...)`，与同一 A6B0 家族对应。

因此模式1 的写入端/消费端算法已经闭合；缺口只剩当前
22份原始参考集中**没有任何正向模式1 紧凑布局条目**。

##### EncryptMode=3：标准 AES-128-ECB 封装 + Windows 历史回退

Windows 写入端 `sub_1000FC10`：

- 密钥 = `MD5(effective_password)`；
- `sub_1000E0A0(key, 0x80, roundkeys)` 是 AES-128 密钥序列；
- `sub_1000ECA0` 是标准 AES 正向块变换；
- 16B 文件密钥恰好一个块，因此没有 IV/链模式：
  `AES-128-ECB(file_key16, MD5(effective_password))`。

Windows 消费端 `sub_10028AB0 case 3`：

- 对密码做 MD5；
- `sub_1002F670 -> sub_1002E7D0/sub_1002F090` 使用 AES-128
  逆密钥扩展 / 逆块变换；
- 解出16B后仍统一由 `UserLogin` 做 FileKeyCRC 校验。

而 `UserLogin` 对模式3 还有明确的历史兼容分支：

1. 先按条目标记的模式3 解包；
2. 若 FileKeyCRC 不匹配，日志输出
   `EncryptMode == eEncryptAESOPENSSL`；
3. 强制以 **模式1** 再解一次同一 16B 封装密钥；
4. 若第二次 CRC 匹配则接受，日志
   `dwKeyCrcOld == m_epiNewInfos[nIndex].FileKeyCRC`。

这说明模式3 标记历史上可能承载过模式1 兼容密文；
Windows 读取端明确做了容错，而不是靠猜测。

Linux 当前 `libcemsfilesyscheck.so::fileKey_Decrypt` 构建只显式实现
模式1/模式2，没有模式3 分支。这是组件能力差异，不应把模式3
误写成“Linux 同样支持”。

##### 严格完成状态

本轮继续向上追 `CreatePartitions` 的 EncryptMode 输入，确认模式1/模式3
不是不可达兼容代码：

- `CUsbRegsiter` 构造函数 `0x10038EB4` 默认令 `this+0x6EC=2`；
- 唯一设置函数 `sub_1003B4E0(arg)` 直接覆盖该成员；
- `WriteNormalULabel` 读取请求 `arg+0x7E8`：值1→设置函数(1)，值2→设置函数(3)，其它→设置函数(2)；
- `CreatePartitions` 三组条目写入端都按 `this+0x6EC` 分派模式2/1/3，
  并把同一个模式字节写入 `entry+0x58 EncryptMode`；
- `UsbtoolBusMgrInter::LabelInfo::Print` 把 `LabelInfo+0x7E8`
  明确打印为 `crypt=%d`；
- 制标 UI 的 `tabAlgorithmComboBox` 初始化为 `SMS4/AES/AES_CROSS`
  三项，默认索引=0，`currentIndex()` 直接写
  `normalDetail+0x44`；策略日志把该字节命名为
  `normalDetail.algorithm`。

继续回到 `cemssafeudisklabeltool.exe` 的实际请求转换链后，UI 到底层 `crypt` 的
桥接也已经直接闭合，不再需要靠字段名相似推断：

- `WriteLabel` 从 `tabAlgorithmComboBox::currentIndex()` 直接写 `normalDetail+0x44`；
- `sub_42E8E0(normalDetail, LabelInfo)` 在 `0x42E8E0` 的转换链中明确执行
  `LabelInfo+0x7E8 = normalDetail+0x44`；反向 `sub_42EC90` 又执行
  `normalDetail+0x44 = LabelInfo+0x7E8`，证明这是同一个正式字段的双向映射；
- `QComboBox` 初始化汇编 `0x47780F..0x47788C` 先构造 `AES_CROSS/AES/SMS4`，
  但传给 `QStringList` 时按栈顶顺序实际依次追加 `SMS4 -> AES -> AES_CROSS`，
  随后 `setCurrentIndex(0)`；因此 UI 索引 0/1/2 精确对应
  `crypt=0/1/2`；
- 结合 `WriteNormalULabel` 已确认的分派：`crypt=1 -> mode1`、
  `crypt=2 -> mode3`、其它（含默认0）`-> mode2`，最终得到
  最终映射为：`SMS4(index0) -> EncryptMode=2`、`AES(index1) -> mode1`、
  `AES_CROSS(index2) -> mode3`。

因此底层 `crypt`、UI `normalDetail.algorithm`、`LabelInfo+0x7E8` 与写入端
模式1/2/3 的可达性/数值映射现在全部闭合。

22份原始真实设备参考集中：

- 44条需要16B 封装密钥的 type2/type4 条目 **全部 EncryptMode=2**；
- 模式1 正向样本：**0**；
- 模式3 正向样本：**0**。

为排除“旧备份里其实已有正例但因设备 ID 缺失而被漏解”的可能，先前已对
`utils/backup` 与 `nopwd_tool/backup` 做过基于文件名/`.meta.json` 身份的复核。
本轮进一步取消这个依赖：对 `/Users/zhangyuxi/Desktop/u_disk` 下所有大小至少13扇区、
不超过1GiB的文件做只读统计集，共扫描 **3896** 个候选。每个候选先用固定 LBA6
滚动密钥解出 `LBA6+0x100 m_crcUsbID[0]`；这个 DWORD 本身就是
`CRC32(device_id)`，可直接作为 LBA12 A6B0 密钥，因此即使文件名和旁挂文件完全没有
设备 ID 也能独立尝试解 LBA12。只把解密后条目0 魔数=`EDPF`、条目数量=1..3
且每条0x60 步长条目魔数均有效的捕获纳入统计。结果为：

- **58** 份有效 EDPF 捕获；
- 49份模式元组=`[0,2,2]`；
- 9份模式元组=`[2,2]`；
- **模式1/模式3 命中仍为0**。

这批58份包含历史和转换状态，只用于扩大物理真实设备配置类型搜索，不扩大22份
严格原始参考集；它仍然证明截至现有实盘语料，模式1/模式3 没有物理
采集。随后新增的证据与这项统计集分开记录：

- 使用官方当前 `CEMSUsbRegsiter.dll`，在不触碰任何物理原始设备的 Unicorn
  虚拟盘测试框架中直接调用 `CUsbRegsiter::CreatePartitions/sub_1003DB50`；
- 测试框架只替换 WinAPI/SEH/配置/挂载后处理等环境边界，并固定 `CoCreateGuid` 作为
  确定性熵；**没有桩函数** `sub_10001190`、`sub_100036E0`、
  `sub_10011010`、`sub_1000FC10` 或 `BuildSector12/sub_10014F30`；
- `this+0x6EC=1/2/3` 时，最终条目0 封装调用点分别只命中对应的
  模式1 `0x1003ED77`、模式2 `0x1003ED44`、模式3 `0x1003EDA7` 各1次；三次均
  `CreatePartitions ret=0`、无模拟器 crash，且原生 `BuildSector12` 写出完整512B密文；
- 三份输出使用同一设备 ID、同一密码 `ProofPass1!`、同一确定性 GUID 流，
  解开外层 LBA12 后 `UserKeyCRC=0xE5A095A1`、`FileKeyCRC=0xFF4C1D36` 完全相同，
  只有 EncryptMode/16B 封装密钥按算法变化；
- 模式1 以独立 A6B0 读取端、模式2 以标准 SM4-ECB、模式3 以标准 AES-128-ECB 解包，
  三者都恢复同一文件密钥
  `147196f5a2ec7912edf13f75d766cb42`，其 `CRC32_bare=0xFF4C1D36`；
- 三份512B输出已作为 `official_virtual_writer_mode{1,2,3}_lba12.hex` 固化，CI
  `lba12_official_virtual_writer_executes_mode1_mode2_mode3_wrapping_paths` 独立验证结构、
  三种解包与 FileKeyCRC。

这里没有把虚拟测试夹具冒充物理真实设备采集；22份严格原始样本和扩大
统计集的统计保持原样。但对“字节含义是否闭合”的标准而言，**一方可执行文件自身
动态生成盘面字节 + 已闭合的 UI 可达链 + 独立消费端往返验证** 已经消除了缺实盘
模式1/模式3 所代表的语义不确定性，而且比 edpcli 自己按逆向公式生成合成向量更强。因此
`+0x38..+0x47` 三条共48B从部分闭环升 **完全闭环**；`oldSM4` 仍只是模式2 的
实现分支选择，不形成第二盘面配置类型。

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
- 时间窗/次数20B已经重新由官方写入端/消费端闭合：
  - Linux DWARF `tagEdpEDiskTmpUse@edpdiskglobal.h:481` 给出正式字段名；
  - Windows `CUsbRegsiter::SetTempUse` 真实机器码将开始/结束时间字符串解析为
    两个64位时间值，并把请求 `+0x40` 原样写入 `useCount`；
  - `BusManageImp::WriteNormalULabel` 的机器码在特殊 OutManage 关闭模式明确把
    临时使用请求次数写为 `0xFFFFFFFF`，普通模式则从业务请求 `+0x947` 取值；
  - Linux `CheckTempUse` 将 `useCount=0xFFFFFFFF` 当作无限次数哨兵：
    不递减、不回写；0表示次数耗尽；其它正值减1并回写；
  - `ullBTime/ullETime` 与 `time(NULL)` 比较，0表示对应时间边界不启用。
- `reverse[104]` 本轮继续拆到写入端覆盖边界，并把前102B的真实保留底层字节来源追到上层调用栈：
  - `CUsbRegsiter::SetTempUse` 先对 EETU 魔数后的 `0x7C` 字节整体
    `memset(0)`；
  - 随后只执行 `memcpy(EETU+0x18, request+0x44, 0x66)`，即覆盖
    reverse 前102B / LBA9 `+0x18..+0x7D`；
  - reverse 最后2B / LBA9 `+0x7E..+0x7F` 没有任何后续覆盖，因此保留
    明确的写入端零初始化值；
  - 对 `BusManageImp::WriteNormalULabel/sub_100A99F0` 的真实机器码重新扫描：
    调用 `SetTempUse(&var_BD4)` 前只写开始/终点字符串和
    `useCount@var_B94`；请求布局因此精确为 `开始[32] / 终点[32] /
    useCount@+0x40 / 调用方保留底层字节@+0x44`；
  - 该函数之前确实调用 `sub_1009BAD0()`，但机器码明确先执行
    `lea ecx,[ebp-0x9EC]`，而 `sub_1009BAD0` 只做 `memset(ecx,0,0x996)`；
    其清零范围为 `ebp-0x9EC..-0x56`，**不包含**位于 `ebp-0xBD4` 的
    SetTempUse 请求。因此 `request+0x44..+0xA9` 102B 不是遗漏的保留全零，
    而是正式的 **写入端未初始化的不透明保留底层字节**；
  - Windows 运行时 `ReadTempUseInfo/sub_10013490` 会把完整0x80 EETU解密并缓存到
    `CEdpDiskControl+0x1076`；对该对象区的精确引用审计显示，
    `GetTempUseInfo/CheckTmpUse` 只读取 `ullBTime/ullETime/useCount`，显式字段引用
    截止 `+0x108A`，而 reverse 从 `+0x108E` 开始没有独立业务读点；
  - 登录成功需要扣减临时使用次数时，`UserLogin` 只执行
    `*(this+0x108A)--`，随后 `WriteTempUseInfo/sub_10013770` 将完整0x80 EETU重新
    加密写回，因此 **运行时 preserves 该完整 0x80 EETU while 仅 consuming
    time/useCount**；Linux `CheckTempUse` 也不读取 reverse；
  - 已提交原始样本与扩展历史只读扫描当前仍未发现非零 reverse：严格非零
    LBA9 配置类型的 EETU 均为 `reverse[104]=0`。这里的零只作为真实配置类型证据，
    不再被误写为协议固定值。

因此本轮把 **LBA9 +0x18..+0x7D 共102B** 从部分闭环升完全闭环：正式
`reverse[104]` 边界、写入端未初始化来源、运行时透明保留 /
负语义消费端与原始实盘四项均已闭合；未来出现非零保留底层字节必须原样保留。
此前 `+0x7E..+0x7F` 2B 已由 SetTempUse 显式零初始化独立闭合，所以现在整个
`reverse[104]` 都是完全闭环，但前102B与末2B的写入端语义不同，禁止重新合并为
“104B 全零填充”。
- `EPPE` 位于 `0x180..0x1ff`，是独立 128B A6B0 区，计数器从 0 重新开始；当前 6/6 解密为 `EPPE 08 00 00 00` 后零填充。
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

因此当前写入端对 `EPPE+0x08..+0x7F` 的120B不是“碰巧为零”，
而是**显式写入端负责的全零尾部**：

- 先把魔数后的124B全部清零；
- 再只覆盖 `+0x04` 的最小密码长度；
- 内层 `sub_10042720` 对完整0x80B加密，并只替换 LBA9
  `+0x180..+0x1FF`。

严格22份原始参考中共有6份 EPPE，重新逐盘复算：

- 6/6 `minPassLen = 8`；
- 6/6 解密后 `+0x08..+0x7F == zero[120]`。

新增 CI 门禁：

- `real_eppe_samples_keep_the_current_writer_zero_tail`。

继续追当前接口边界后，消费端侧已经闭合到“只有 minPassLen 有语义”：

- 正式注册 API `CUsbRegsiter::GetPassInfoEx` 读取/解密完整0x80B、校验
  `EPPE` 后，**只执行 `*out = *(decoded+0x04)`**；尾部不向调用者暴露；
- 独立 `modfilesyscheck::ReadMinPassLenInfo` 同样只消费魔数与 `+0x04`；
- 两套独立 `EdpDiskCtrl` 确实都保留了可搬运完整0x80B的
  `ReadPassExInfo -> CEdpDiskControl::GetPassExInfo` 兼容辅助函数，
  但 DLL 对外只导出 `CreateEdpEDiskCtrlIntObj/ReleaseEdpEDiskCtrlIntObj`；
  当前工厂返回对象的虚表 `0x1008021c` 只有15个方法，未包含
  `GetPassExInfo/sub_10022AF0`。两套反编译文本中该辅助函数也只出现实现和
  相邻 thunk，没有形成当前产品的值相关尾部消费端。

因此这里不是“因为没搜到字段名就猜保留”，而是已经具备明确当前
写入端负责的全零来源 + 两套语义读取端负消费证据 + 当前
公共接口 边界 + 原始实盘四项证据。

结论：

- `LBA9 +0x188..+0x1FF` 120B：部分闭环 -> **完全闭环**；
- 完全闭环的含义是 **EPPE 写入端负责全零尾部** 在当前已知代际的存储/消费
  行为已经闭合，不表示未来历史配置类型必须为零；
- 兼容读取若遇到未知非零尾部，应保留/报告，不得以本结论为由主动清零。
- SAPF 位于 `+0x100..+0x11f`，整段按字节 `^0x88` 还原。其
  `+0x04..+0x13` 是一个完整 16B MBR 分区条目，但**不是当前 LBA0 分区项的镜像**：
  当前 14 份 SAPF 样本中 14/14 均与当时 LBA0 `0x1be..0x1cd` 不同。
- `UDiskLabelRepair.dll::CLabelRepair::Repair` 在 LBA0 无效时先检查 LBA9 SAPF；
  `Repair0Sector(from sector 9)` 会把 SAPF 的四个 DWORD 直接写到新 MBR
  `0x1be/0x1c2/0x1c6/0x1ca`，随后写回扇区 0；若该路径失败才尝试尾部备份扇区。
  因此 SAPF 可闭合为 **LBA0 第一分区项的恢复模板/备份项**，不能再描述成“当前 MBR 副本”。

本轮继续把 SAPF 与 LBA9 中间区域按**真实物理边界**拆开，而不是把
`+0x080..+0x17F` 继续整体记未知。

##### 当前注册/运行时对 LBA9 中间256B的写边界：BuildSector6 有跨扇区 side effect

当前 `CUsbRegsiter::RegsiterUsb/sub_1003B560` 的机器码/反编译控制流明确：

1. 先从元数据基础版本一次读取完整13扇 `LBA0..LBA12` 到工作缓冲；
2. 注册过程中显式调用 LBA4、LBA6、LBA8、LBA11 构造器；
3. `sub_1003DB50` 只向 `sector_size*7` 与 `sector_size*12` 写入，
   即只重建 LBA7/LBA12；
4. `BakupUsbSec/sub_10040940` 只是把现有缓冲复制到尾部备份位置，
   不修改工作缓冲；
5. 最后把完整13扇工作缓冲写回。

此前据此写成“当前注册路径不拥有 LBA9”是不完整的。重新下钻
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
因此 LBA9 中间256B不是纯原样保留区，而是长 Dept/User 续段与
SAPF/历史保留底层字节复用的多配置类型物理区。
运行时两个独立设置函数又进一步把所有权边界锁死：

- `SetTempUse`：读取→修改→写入 LBA9，但只替换 `+0x000..+0x07F`；
- `SetPassInfoEx`：读取→修改→写入 LBA9，但只替换 `+0x180..+0x1FF`。

对应读取函数也分别只读取首/尾0x80；这只能说明 TempUse/PassEx 运行时 API
不会触碰中间区，不能否定 BuildSector6 的跨扇区续段。

##### `+0x080..+0x0FF`：正式长 Dept 续段，含 join=60 / join=59 双配置类型

严格参考中有8盘该128B非零，实际每盘只有16或17B非零。尝试以设备 ID CRC
把它作为独立0x80 A6B0块解密，结果无任何可识别魔数/结构；而原始字节直接呈现
GBK文本特征。

最关键的跨扇区交叉：

- 4份具有完整76B ELABEL `Dept=` 的真实盘，
  LBA9 `+0x80` 的16B **逐字节等于 ELABEL Dept 的 `dept[60:]`**；
- 另4份 ELABEL Dept 在63B位置截断，并停在 GBK“建”的首字节 `BD`；
  LBA9 `+0x80` 以 `A8` 开头，随后正好是
  `湖输变电运检中心`，与同一完整部门字符串的后续字节吻合；
- 重新定位官方 BuildSector6/ReadSector6 后，这里已经不是“像 Dept 保留底层字节”的推测：
  当前写入端精确写 `Dept[60..NUL]` 到 LBA9+0x80；
- Linux/Windows/cemsudisk 读取端都有相同兼容逻辑：
  - 内联第60字节非零：把续段接到 Dept index60；
  - 内联第60字节为0：把续段接到 Dept index59，
    专门修复历史配置类型的 差一位/GBK split。

继续追读取端代际后，join59/60 的选择条件已经从“兼容分支”细化到机器码级。当前
`CEMSUsbRegsiter.dll::ReadSector6/sub_100152A0@0x1001566F..0x100156D6` 的实际流程是：

1. 标记命中后，把标记后的 **0x3C=60B** 复制到局部前缀；
2. 读取 `prefix[0x3B]`，也就是第60个内联字节 / Dept[59]；
3. 若该字节为0，LBA9+0x80 的0x80B 续段写回 `Dept+0x3B`；
4. 若非0，则写回 `Dept+0x3C`。

所以这里没有隐藏版本/配置类型标志：盘面镜像自己用内联第60B是否为 NUL 决定接缝。

更重要的是，本机 `VRV/cems/Edp/fileophook.dll` 与 `fileophook64.dll` 给出了更早一代
读取端。两者 PDB 分别落在
`\\SVNRoot\\vrvrsms2.0\\Cems2.0\\trunk\\modCems\\Bin\\FileOpHook.pdb`
和 `FileOpHook64.pdb`。旧32/64位机器码在标记分支中都先复制60B 前缀，
随后**不做前缀[59] 分支，直接把续段写到前缀基址+0x3B**，即固定
join=59。公开的2019 CEMS样本文件清单还把**同一产品目录路径**的
`cems\\edp\\fileophook*.dll` 与
`cems\\edp\\safeudisklabeltool\\cemsusbregsiter.dll` 放在同一软件包里；这只能作为
CEMS2.0/EDP 产品线共包证据，**不能**证明该2019归档中的钩子与本机2022编译的
`FileVersion=1.0.0.11` 二进制哈希相同。

本轮继续排除了“这两份读取端 DLL 自己还藏着 join59 原始写入端”的可能路径：

- x86 `fileophook.dll` SHA-256 为
  `db3d0a94694cbed20696ef00b8f22e8e111e3f12068c763efc3e703fb960bc65`，PE 编译时间
  `2022-12-13 03:40:57`，固定文件版本为 `8.1.2211.2811`、字符串
  `FileVersion=1.0.0.11`。标记 `0x40245E2A` 在该二进制只有一个命中，正是
  `fcn.10022F80@0x100232ED` 的 `cmp`。唯一有实际代码交叉引用的
  `\\.\\PhysicalDrive%u` 路径进入 `fcn.10026280`，该函数在 `0x1002631B`
  以 `dwDesiredAccess=0x80000000 (GENERIC_READ)` 打开物理盘，随后只执行
  `SetFilePointer + ReadFile`。模块虽导入 `WriteFile`，但这些调用点没有连接到这一唯一
  实际 PhysicalDrive 原始路径，不能据此推导出扇区写入端。
- x64 `fileophook64.dll` SHA-256 为
  `93364d3f6798570fc10345cfe30d46d8b86b68f8e3468c49fa76ae7e8a42d2a9`，PE 编译时间
  `2022-12-13 02:44:33`，版本同样为 `8.1.2211.2811 / 1.0.0.11`。该架构也只有一个
  标记命中：`fcn.180026D70@0x18002724F` 的 `cmp`；其固定 join59 分支在
  `0x18002726C` 复制 `0x3C` 字节，再于 `0x180027277` 把续段写到
  前缀 `+0x3B`。唯一 `\\.\\PhysicalDrive%u` 交叉引用则进入 `fcn.18002B880`，
  `0x18002B8FE` 同样使用 `GENERIC_READ`，最终只调用 `ReadFile`。
- 对整个 `/Users/zhangyuxi/Desktop` 与 `/private/tmp` 的 PE 文件按标记原始字节
  `2A 5E 24 40` 扫描，本轮只命中11个已知家族文件；额外出现的 2024
  `EdpEDiskCtrl.dll` 两个命中也都是兼容读取端的 `cmp`，没有出现新的历史标记写入端。
  `VRV.zip` 中的 Edp FileOpHook 仍是同一 2022 尺寸/代际，ydcc BusManage/CEMSUsbRegsiter
  则是当前 2026 树，因此归档也未补出更早写入端。

因此旧版 join59 现在可以严格描述为 **CEMS2.0 旧代标准读取端 ABI**，
而当前 ydcc 通过内联 NUL 自描述同时兼容59/60。仍缺的是同代
`safeudisklabeltool\\cemsusbregsiter.dll` 的原始写入端字节；没有写入端之前不能把
LBA6+0x3F 或 LBA9 续段误升完全闭环。

严格22份完整统计集：

- 14/22 未触发长 Dept 标记；
- 8/22 触发标记且 LBA9+0x80 非零；
- 其中4/8为当前 join=60：重建76B Dept，续段含NUL共17B；
- 另4/8为旧版 join=59：LBA6 内联在 GBK 前导 `BD` 后出现NUL，
  LBA9从尾字节 `A8` 开始；官方读取端从 index59 覆盖后同样重建出
  完全相同的76B合法GBK Dept，续段含NUL共18B。

CI 新增
`lba9_dept_continuation_preserves_both_official_reader_join_profiles`，
并提交一份严格原始 Lexar join59 的 LBA6/LBA9 最小证据夹具。

但严格完全闭环仍差最后一环：已定位的当前 Windows/Linux/vrvaud
三套 BuildSector6 都只生成 join=60，尚未找到4份 join=59 原盘对应的历史
写入端。因此该128B继续 **部分闭环**；阻塞项已缩小为
“仅旧版 join59 写入端未定位”。

##### SAPF `+0x114..+0x11F`：32B 解码范围内的随配置类型变化保留底层字节

`UDiskLabelRepair::sub_10008550` 固定取 LBA9 `+0x100` 起 **0x20B**，
逐字节 `^0x88` 后再检查 `"SAPF"`。所以 SAPF 的物理解码范围明确是
`+0x100..+0x11F`，不是只到已闭合的魔数+16B恢复项。

14份真实 SAPF 对最后12B `+0x114..+0x11F` 重新解码后至少出现5种形态：

- 有全零配置类型；
- 也有 `0xFFFFFFFE`；
- 多组 `0x77xxxxxx` 一类典型32位进程/栈保留底层字节值；
- 同一硬件/配置类型可重复稳定出现同一组尾值。

这些12B不匹配同盘 LBA0 磁盘签名，也不匹配当前 MBR 分区条目。
本轮继续把修复消费端精确到字段级：`sub_10008550` 会把整个32B SAPF
解码并返回；其上层双副本一致性检查只比较解码后 `+0x04/+0x08/+0x0C/+0x10`
对应的 MBR 启动标志、分区类型、起点 LBA、大小，完全不比较 `+0x14..+0x1F`；
真正重建 LBA0 的 `sub_10008620` 也只把 SAPF `+0x04..+0x13` 这16B 分区条目
写回 MBR `+0x1BE`。`vrvaud_c` 的两条快速路径更只解码/检查 `SAPF` 魔数 4B。
因此这12B已有明确的 **结构性复制 / 负向语义消费端** 证据。继续取得
长 User 一方运行时正例后，物理重叠的另一种配置类型也闭合：最大155B User 会由
官方 `BuildSector6` 把续段连续写满 `+0x100..+0x17F`，官方
`ReadSector6` 又完整重组原 User。也就是说 `+0x114..+0x11F` 的两种已知配置类型都已有
完整语义：

- SAPF 配置类型：**无所有者尾部保留底层字节**。真实值至少5种，修复/一致性代码不消费；
- 长 User 配置类型：**有效续段负载**。一方写入端→盘面→读取端已闭环。

因此历史 SAPF 最初写入端不再是这12B的业务语义阻塞项；该区从部分闭环升
**完全闭环**。完全闭环绝不表示 SAPF 尾部应归零，反而要求兼容实现保留任意保留底层字节，
并按配置类型区分长 User 续段。

##### SAPF 后 `+0x120..+0x17F`：长 User 续段 / 原样保留复用区

- SAPF 读取端只解码到 `+0x11F`；
- `vrvaud_c` 的快速检查只取 `+0x100` 的4B 魔数；
- `SetTempUse` / `SetPassInfoEx` 不会覆盖 `+0x120..+0x17F`；
- 但 BuildSector6 在 User>=32 时会从 LBA9+0x100 写
  `User[28..NUL]`，最大可以延伸覆盖整个 `+0x120..+0x17F`；
- 对应 ReadSector6 的 User 标记分支会从 LBA9+0x100 读完整0x80B，
  并支持 join=28 / 旧版 join=27 两种接缝；
- 14/14 SAPF真实盘及独立SanDisk当前都为零。

严格22盘没有 User>=32 的物理正向样本这一事实继续保留，但随后已用隔离虚拟盘直接
执行官方 Windows `BuildSector6`/`ReadSector6` 获得最大155B User 的一方
运行时正向盘面往返验证；写边界精确止于 `+0x17F`。因此这96B已在后续审计中升
**完全闭环**，不能再描述成“缺正向证据”。

新增门禁：

- `lba9_middle_profile_material_must_not_be_canonicalized_to_zero`：
  - 已提交真实夹具必须继续保留 `+0x80` 的非零配置类型；
  - SAPF `+0x114..+0x11F` 必须同时保留全零与非零两类真实反例；
  - 已提交 SAPF 夹具当前 `+0x120..+0x17F` 仍锁定为零观察，
    但文档明确零不是协议要求。

该阶段之后 LBA9 又继续闭合长 User 与 SAPF 尾部；最终以文末严格总表为准，当前为：

```text
384 COMPLETE / 128 PARTIAL / 0 UNKNOWN
```

其中 EETU reverse 保留底层字节已新增102B 完全闭环，随后 EPPE 写入端负责全零尾部
又新增120B 完全闭环；当前128B 部分闭环集中在 Dept join59/join60 续段。

> **附录口径警告：** 以下 LBA10 EESI 段落记录旧研究集合，旧集合曾把第三来源
> SanDisk EESI 作为正例。2026-09-21 已按第1.1节重新冻结当前金标；现行20份唯一
> 金标 LBA10 全零，因此前0x80B 当前严格状态为部分闭环。下方静态代码路径和
> 历史 EESI 解码仍是有效背景证据，但其中任何“升级完全闭环”的阶段性文字均已
> 被第3节主账本覆盖。

##### LBA10 EESI：前 0x80B 的读写边界已闭合

Windows `edpediskctrl.dll` 同时给出读端和写端：

- `GetEdpEdiskSetInfo -> sub_1000f930`：定位 LBA10，读取整扇，但只对前 `0x80` 执行 A6B0 解密并检查 `0x49534545 == "EESI"`；成功后也只向调用者返回这 `0x80`。
- `SetEdpEdiskSetInfo -> sub_1000fc70`：强制写入 `EESI` 魔数，只加密输入结构前 `0x80`；随后先读取原扇区，只替换前 `0x80`，再把整 512B 写回。
- 因此 `0x80..0x1ff` **不是 EESI 自身的填充**。当前写入端明确保留这 384B 原字节；SanDisk 实盘该区恰好全零只能作为样本事实，不能推导协议恒零。

当前严格22份生成参考中的 EESI 实盘解密结果：

- `+0x00..0x03 = EESI`；
- `+0x04..0x07 = 1`；读取端和两套 `EdpEDisk.exe::OnInitDialog` 外部调用方
  都会先把该 DWORD 默认设为1，但卷标设置 `IDOK` 写入端会把完整0x80B
  EESI清零后只写两个卷标，因此保存路径明确把该DWORD写成0；其值相关消费端
  已追到 `UsbSuspensionWnd.dll` 生命周期控制，详见下节；
- `+0x08..0x17`：16B **共享/type2 卷标**；读取端默认字符串与实盘均为
  GBK“交换区”。`UserLogin` 把该槽赋给本地 `std::string`，在 type2 分支
  直接将其 `c_str()` 传给 `SetVolumeLabelA`；
- `+0x18..0x27`：16B **加密/type4 卷标**；读取端默认字符串与实盘均为
  GBK“保密区”。`UserLogin` 在 type4 分支同样将该槽对应字符串传给
  `SetVolumeLabelA`；
- `+0x28..0x7f`：当前 SanDisk 实盘为零。虽然尚未发现字段消费者和正式字段名，
  但它们已经不能继续标未知：读取/集合两端都把完整0x80B结构往返验证，
  所以这88B明确属于 EESI API 负载，只是业务语义未解释。

本轮又补到一份独立历史实盘配置类型：

- `/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`，
  元数据记录 `device_id=disk&ven_netac&prod_onlydisk&rev_0000`，整份6656B
  SHA-256=`3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`；
- LBA6 解码后的 `crcUsbID[0]=0x5088EE37` 与 `CRC32(device_id)` 精确一致，
  双倍保护值、LBA7 紧凑布局 EDPF、LBA12 紧凑布局 EDPF 也都在同一密钥下自洽；
- LBA10 前0x80按 `CRC32(device_id)=0x5088EE37` 解密后再次得到
  `EESI`, `+0x04=1`, `+0x08="交换区"`, `+0x18="保密区"`，
  `+0x28..0x7F` 仍为88B全零；
- 回归门禁 `historical_netac_lba10_confirms_the_same_eesi_head_and_zero_uninterpreted_payload`
  固化其前0x80密文与解密结果。

这份 Netac 捕获明显不是当前标准构造器的简单零模板（LBA6 含真实
旧版/保留底层字节配置类型），因此可作为额外真实设备/配置类型佐证；但其“原始生成来源”
尚未达到主22份参考同等审计强度，所以本轮**不**把它并入严格生成参考计数，也不因为
两份 EESI 的88B都为零就把 `+0x28..0x7F` 升完全闭环。

本轮继续专门追 `+0x04`，先排除了一个很自然但错误的解释，再找到了真实行为链：

- 当前 `CEdpDiskControl::UserLogin/sub_10022F50` 在栈上建立 EESI 输出结构，
  结构基址为 `ebp-0x334`，随后调用 `GetEdpEdiskSetInfo(&var_334)`；
- 因此 `+0x04` 精确对应 `ebp-0x330`，`+0x08` 对应 `var_32C`，
  `+0x18` 对应 `var_31C`；
- 登录函数后续明确读取 `var_32C/var_31C`，并把它们送入 type2/type4
  `SetVolumeLabelA` 路径，但**整个 UserLogin 没有任何 `ebp-0x330` 引用**；
- 所以 `+0x04` 不控制当前登录路径中“是否使用自定义交换区/保密区卷标”；
- `SetEdpEdiskSetInfo/sub_10022920 -> sub_1000FC70` 只是把调用者完整
  0x80 结构写入，除强制魔数=`EESI` 外不解释/改写 `+0x04`；
- 此前“接口虚表 slot8/slot9 没有外部调用方”的结论经继续反查后已经推翻。
  `out_raw_data/EdpEDisk.exe` 的 `CEdpDiskDlg::OnInitDialog` 明确在对象
  `+0x534` 建立0x80B EESI缓冲：先整块清零，写 `object+0x538=1`
  （即 EESI `+0x04=1`），构造 `+0x08/+0x18` 两个默认卷标，再通过
  `(*(**dword_4832bc + 0x20))(object+0x534)` 调用外部读取函数；
- 同一程序的卷标设置对话框 `IDOK` 处理函数 `sub_4132B0` 则
  `memset(&var_9C,0,0x80)`，只填写 `var_94=+0x08` 与 `var_84=+0x18`，
  最后由 `sub_413280 -> vtable+0x24` 调 `SetEdpEdiskSetInfo`。因此这条官方
  写入端确定写出 `+0x04=0`；
- 独立 `VRV/cems/ydcc/edpedisk.exe` 完整复现同一模式：
  `CEdpDiskDlg::OnInitDialog` 写 `+0x04=1` 后虚表+0x20 读取，设置对话框
  `sub_4152C0` 清零0x80后只填两卷标，`sub_415290 -> vtable+0x24` 集合；
- 两套程序 SHA-256 分别为
  `cfa1317775801381b6ca51f13857d1e52506ff48ac4df94d7b9f91f742d3e4a1` 与
  `dc71c30041c4fe9fab277737116216502e9f6a610630a1a70b125c59441e1fd1`，
  不是同一文件副本；
- 消费端侧，两套程序均 `LoadLibraryA("UsbSuspensionWnd.dll")`，并解析
  `Show/Destroy/SetParentWnd`。读取 EESI 后，`+0x04==0` 分支调用该辅助函数的
  `Destroy`；自动登录成功后，`+0x04!=0` 且辅助函数初始化标志为真时进入
  刷新/`Show` 链。因此这是明确的值相关行为消费端；
- 较旧 `/VRV/edp/EdpEDiskCtrl.dll` 与 SHA 不同的中间版本
  `/VRV/cems/Edp/edpediskctrl.dll` 都没有 `0x49534545(EESI)` 读写路径，
  说明该设置结构属于后续新增功能，不能借旧版行为反推 `+0x04`。

因此 `EESI+0x04` 的严格闭环已经成立：4B边界确定；官方写入端同时存在
默认/启动路径写1与设置保存路径写0；官方消费端对0/非0执行不同的
`UsbSuspensionWnd` 生命周期行为；唯一启用 EESI 的原始 SanDisk 实盘值为1。
该DWORD由部分闭环升为 **完全闭环**，文档采用保守的行为命名
**UsbSuspensionWnd 生命周期/控制标志**，不臆造未恢复的原始 C++ 成员名。

本轮进一步把 LBA10 后续区域从“未知”拆成两个不同的部分闭环边界。

##### `+0x28..0x7F`：EESI 调用方负责的兼容扩展

当前 `edpediskctrl.dll` 与独立另一版
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
设置函数可以保存调用者提供的这些字节，读取函数会原样返回解密后的这些字节。
当前 `UserLogin` 对本地 EESI 输出结构只读取 `+0x08/+0x18` 两个卷标，
没有读取这88B。两套独立 `EdpEDisk.exe::OnInitDialog` 外部读取函数调用方同样
只消费两个卷标；两套卷标设置 `IDOK` 处理函数将完整 0x80B EESI 负载清零，
只填写两个卷标再经虚表+0x24 集合，因此当前 UI 写入端对这88B的来源为零。

继续把调用面按“谁拥有这88B”而不是“必须猜出88B内部字段名”重新审计后，
这一区域可以严格闭合：

- 两个独立 EESI `EdpEDiskCtrl` 构建都把 `+0x28..0x7F` 与其它
  EESI 字节一起完整读取/集合，底层不生成、不解释、只做结构性往返验证；
- 两套独立 `EdpEDisk.exe` 官方调用方都在保存前把完整0x80B清零，
  只填写 `+0x08/+0x18` 两个卷标，因此当前官方调用方负责
  配置类型对这88B明确写零；
- 当前 `UserLogin`、两套 `OnInitDialog` 读取函数调用方以及已扫
  `UsbSuspensionWnd` 行为链均没有对这88B做值相关读取；
- 更老两个独立 `EdpEDiskCtrl` 构建根本没有 EESI 读取/集合，说明不能把
  88B误解释成继承自旧协议的隐藏活动字段；
- 严格 SanDisk EESI 与独立历史 Netac EESI 两个正向实盘配置类型的88B均为零，
  且已有回归锁定。

因此这88B从部分闭环升为 **完全闭环**，语义命名为
**EESI 调用方负责的兼容扩展**。这里的完全闭环表示生命周期已经
闭合：API 调用方可携带任意未来扩展字节，底层必须往返验证；当前官方调用方
写零且当前业务消费端不解释。它**不**表示未来值必须为零，也不把未知内部结构
冒充成 `Reserved[88]`。

##### `+0x80..0x1FF`：不属于 EESI 的不透明保留物理尾部

两版设置函数都明确采用读取→修改→写入：

- 先读取完整512B LBA10；
- 只覆盖前0x80B EESI密文；
- 后0x180B原样保留；
- 再写回完整扇区。

两版读取函数也都只解密/返回前0x80B，从不暴露后384B。
所以后384B不是“EESI 填充”，而是 **EESI 写入端不拥有、
只负责原样保留现有的共存物理尾区**。

继续做跨代代码所有权审计后，结论可以再提高一级：

- 两个独立 EESI 构建（`ydcc/edpediskctrl.dll` 与
  `out_raw_data/EdpEDiskCtrl.dll`）设置函数都采用完全相同的
  读取→修改→写入：读取完整0x200B，只替换前0x80B，再原样写回后0x180B；
- 两个读取函数都只解密/返回前0x80B；
- 更老 `VRV/edp/EdpEDiskCtrl.dll` 与
  `VRV/cems/Edp/edpediskctrl.dll` 两个独立构建连
  `EESI` 魔数 / 读取 / 集合路径都不存在，因此同样没有该尾部的写入端
  或消费端所有权；
- 当前收集到的其它产品组件没有找到 LBA10 尾部的独立解析入口。

真实盘交叉验证也从22份扩展了一层：除已提交原始样本与独立 SanDisk 外，
对本机历史语料用 **LBA6 crcUsbID 保护值 + LBA12 解密后 EDPF 魔数** 双重过滤，
得到58份有效 EDP 前部快照；58/58 的 `LBA10+0x80..0x1FF` 均为零，
其中2份独立 EESI 正例也都是 tail384B 全零。

这里的完全闭环 **绝不表示协议要求384B恒零**。恰恰相反，官方设置函数的
原样保留现有行为说明：若未来遇到非零历史/共存配置类型，兼容实现必须
原样保留，不能清零。

因此 LBA10 的严格账本从：

```text
36 COMPLETE / 4 PARTIAL / 472 UNKNOWN
```

先调整为：

```text
36 COMPLETE / 476 PARTIAL / 0 UNKNOWN
```

随后跨代所有权 + 读取函数负向消费端 + 扩展实盘验证闭合后，
`+0x80..0x1FF` 384B 再从部分闭环升为 **完全闭环**。因此当前 LBA10 为：

```text
424 COMPLETE / 88 PARTIAL / 0 UNKNOWN
```

剩余88B只有 `+0x28..0x7F`；`+0x04..0x07` 已由两套独立官方 UI 的0/1
写入端、`UsbSuspensionWnd` 值相关消费端与原始 SanDisk 实盘闭合为完全闭环。
后88B虽可完整往返验证，且当前 UI 写入端明确写零，但仍缺字段划分、
非零配置类型与值相关消费端。

`UserLogin` 的实际汇编还明确给出对象映射：`+0x08 -> ebp-0x74` 的
`std::string`，`+0x18 -> ebp-0x54` 的 `std::string`；type2/type4
分支分别以这两个对象调用 `SetVolumeLabelA`。另一版
`out_raw_data/EdpEDiskCtrl.dll` 也存在同构路径。因此两个16B字段的最终运行时
用途已经闭合为交换区/保密区卷标，不再只是“文本槽候选”。

#### LBA0 旧版 MBR 消息指针字节：+0x1B5..+0x1B7 闭合

本轮把原先笼统归为“446B 引导代码”的尾部重新逐字节拆开。
官方 Windows `UsbMainBSec @ 0x100E7220` 在：

```text
+0x1B5 = 0x2C
+0x1B6 = 0x44
+0x1B7 = 0x63
```

这三字节不是普通保留值，而是旧版 MBR 引导代码的三个消息指针低字节。
消费链可直接由同一官方模板反汇编闭合：

1. 模板开头把源 `0x7C1B` 起的 `0x1E5` 字节复制到 `0x061B`，
   然后 `retf` 到复制后的代码执行；
2. 因此原扇区偏移 `X` 在运行时映射为绝对地址 `0x0600 + X`；
3. copied 引导代码中三处：

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

写入端侧也明确：`sub_10013FD0` / 旧版完整 MBR 路径会从
`0x100E7220` 整体复制 `UsbMainBSec`，因此这三字节随模板固定生成。
当前注册路径则会清零 `+0x000..+0x18F`，但不覆盖
`+0x190..+0x1BD`，所以旧模板尾部可能继续保留，即使引导代码正文
已经被清掉。

22份原始真实设备参考集的逐盘统计只有两种状态：

- **14/22 = `2C 44 63`**；
- **8/22 = `00 00 00`**；
- 无第三种值。

CI 原始夹具同时保留两种配置类型。零态表示该旧版尾部不存在/已清空；
`2C 44 63` 态的写入端、消费端、目标字符串和实盘都已闭合。
因此 `+0x1B5..+0x1B7` 共 **3B 部分闭环 -> 完全闭环**。

相邻区域不随之升级：

- `+0x1A0..+0x1A3`：旧版 `BuildSector0/sub_10013F10` 明确写
  SectorSize；22盘有4份为512，其余为0。该字段后续已由可选覆盖项生命周期、
  跨组件负向消费端与历史 0/512 双配置类型进一步闭合，见后文独立小节；
- `+0x1B8..+0x1BB`：已经闭合为标准 MBR 磁盘签名写入端：
  `GetSystemTimePreciseAsFileTime`（回退 `GetSystemTimeAsFileTime`）
  -> FILETIME 转 Unix 秒 -> low32 -> `CREATE_DISK_MBR.Signature`
  -> `IOCTL_DISK_CREATE_DISK`。22/22 非零、19种值，同一 onlyid 重复备份稳定。
  Windows 驱动器布局 API 会把它作为 MBR 签名报告，但当前已审 EDP
  `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 调用均未发现业务逻辑读取该值，因此按本项目
  “EDP 消费端也需闭合”的严格口径继续部分闭环；
- `+0x1BC..+0x1BD`：在这一阶段仅确认官方模板和22盘均为0，因此当时仍未升级；
  后续已继续补齐当前 SAFE6/Linux 写入端所有权边界、引导代码/EDP 负向
  消费端与57份历史快照，最新结论见下方独立闭环小节。

#### LBA0 引导代码配置类型：当前 0x190 清零与旧版 UsbMainBSec 模板已分型

继续沿当前 Windows 注册主链回溯后，LBA0 前400B已经不再只是“观察到有零态”。
`CUsbRegsiter::RegsiterUsb` 在各扇区构造器与分区构造完成后、最终
`WriteSectorData(..., count=0x0D)` 之前，**无条件**执行：

```text
memset(metadata + LBA0 + 0x000, 0, 0x190)
```

因此当前注册写入端对 `LBA0+0x000..+0x18F` 的正式输出就是全零[400]。
独立的 `UDiskLabelRepair.dll::CLabelRepair::ReCreate0Sector` 新建路径也在
`sub_10003960` 中先清零同一0x190B，再清零0x40B MBR 表并重建分区项与
`55 AA`。这两条独立官方路径把“当前全零引导代码”从样本现象升级为
明确写入端行为。

另一方面，`cemsusbregsiter.dll` 自身保留正式静态
`UsbMainBSec@0x100E7220`。对其前0x190B独立提取后的 SHA-256 为：

```text
4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed
```

21份非转换 `nopwd_tool/backup` 原始完整快照重新聚类后，前400B只有三类：

- 多份旧版原盘与上述 `UsbMainBSec` 前400B **逐字节完全一致**；
- 多份当前原盘为完整全零[400]；
- Aigo L8302 为单独的第三种引导代码配置类型（既非上述模板也非全零）。

仓库筛选后的原始协议夹具已经同时覆盖第一、第二类，并新增
`lba0_bootstrap_profiles_are_zero_or_the_official_usb_main_bsec_prefix`
回归门禁。该门禁还固定当前已观测的尾部边界：

- `+0x190..+0x19F` 为零；
- `+0x1A0..+0x1A3` 仅见 SectorSize = 0 或 512；
- `+0x1A4..+0x1B4` 为零；
- `+0x1B5..+0x1B7` 是已闭合的旧版消息指针；
- `+0x1B8..+0x1BB` 是标准 MBR 磁盘签名；
- `+0x1BC..+0x1BD` 为零。

此前这批证据**没有增加严格完全闭环字节数**。原因是：当前全零写入端虽已闭合，
标准旧版模板的静态身份也已由多盘逐字节证实，但还没有定位到“哪一代官方
注册/格式化写入端将这400B 模板写入 LBA0”的历史写入端，也没有解释
Aigo L8302 第三种引导代码的写入端/选择条件；尾部 SectorSize/保留的
业务消费端也未全部闭合。因此 `+0x000..+0x1B4` 继续整体保持部分闭环，
当时总计仍为 **3656B 完全闭环 / 3000B 部分闭环**；后续对尾部逐段拆分后的最新统计见下文。

##### LBA0 `+0x190..+0x1B4` 再拆分：33B 无所有者兼容区域闭合

继续按 SAFE6 实际调用链复核后，先纠正一个旧归因：Windows
`BuildSector0/sub_10013F10` **只在 SAFE1 分支**由 `RegsiterUsb` 调用；SAFE6 主链
不会调用它。SAFE6 在完成 LBA4/LBA6/LBA8/LBA11/EDPF 等构造后，最终只执行：

```text
memset(LBA0 + 0x000, 0, 0x190)
WriteSectorData(LBA0..12, count=13)
```

因此 `+0x190..+0x1BD` 在当前 SAFE6 中不是新建负载，而是既有盘面保留底层字节。
Linux `CLabelManage::BuildSector0@0x1C8CC, diskfile.cpp:625` 独立证明同一边界：函数
只根据 `m_nSectorSize` 计算 MBR 分区扇区数量、清/写 `+0x1BE` 的64B分区表，
完全不写 `+0x190..+0x1BD`。

把尾部按真实行为拆开后：

- `+0x190..+0x19F` 16B：旧版 `UsbMainBSec` 固定为零；Aigo L8302 的
  `Netac_USB_API.dll::sub_10003880` 整扇模板也为零；当前 SAFE6/Linux 构造器
  都不拥有该区，只原样保留；
- `+0x1A0..+0x1A3` 4B：Windows SAFE1 `sub_10013F10` 明确写扇区大小，
  旧版 `UsbMainBSec` 为512；但 Linux BuildSector0 只使用扇区大小计算分区、
  不把它落到此槽，当前 SAFE6也只原样保留，因此这是独立兼容字段；
- `+0x1A4..+0x1B4` 17B：与前16B相同，旧版/Netac 写入端均为零，当前
  SAFE6/Linux 写入端均不拥有。

消费端侧也逐段核对：`UsbMainBSec` 16 位引导代码的明确尾部数据引用只有
`+0x1B5/+0x1B6/+0x1B7` 三个消息指针以及标准 MBR 分区/签名；当前注册准入、
`UDiskLabelRepair::ReCreate0Sector/sub_10003960` 也不解析上述两段33B。修复的新建
路径同样只清前0x190和`+0x1BE`分区表，证明这33B不属于修复负载。

实盘方面，严格22份原始参考的两段33B均22/22全零；进一步只读扫描
`nopwd_tool/backup + utils/backup` 的57份完整历史快照仍为57/57全零。相邻
SectorSize 则明确出现双配置类型：扩展57份为34×512、23×0，证明不能把整个尾部
机械叫做全零填充。

因此本轮先升级真正闭合的两段：16B+17B = **33B 部分闭环 -> 完全闭环**，语义为
**跨配置类型无所有者原样保留 / 历史全零兼容区域**。完全闭环不表示
未来必须为零；若发现非零未知配置类型，兼容实现应原样原样保留。SectorSize 4B 因
SAFE6 历史0/512选择条件及值相关消费端仍缺，继续部分闭环。

##### LBA0 `+0x1BC..+0x1BD`：2B 标准 MBR 保留 / 无所有者兼容字闭合

这2B此前因为“模板为0 + 实盘全零”不足以满足严格口径而保持部分闭环。继续沿与
`+0x190..+0x19F/+0x1A4..+0x1B4` 相同的所有权/消费端方法复核后，证据补齐：

- 当前 SAFE6 `RegsiterUsb` 只清 `+0x000..+0x18F`，随后从 `+0x1BE` 起重建分区表，
  因而 `+0x1BC..+0x1BD` 属于读取前保留底层字节，当前写入端不拥有；
- Linux `CLabelManage::BuildSector0@diskfile.cpp:625` 同样只处理 `+0x1BE` 起的 MBR
  分区条目，不写该2B；
- 旧版 `UsbMainBSec` 与 Aigo/Netac 的完整 MBR 模板在这2B均为 `00 00`；
- 16 位 `UsbMainBSec` 引导代码已确认的尾部直接引用集中在三个消息指针、
  分区表与签名，不读取 `+0x1BC/+0x1BD`；当前注册/准入、
  `UDiskLabelRepair::ReCreate0Sector` 与已审驱动器布局路径也不赋予这2B业务语义；
- 严格22份原始参考 22/22=`00 00`；扩展57份完整历史快照同样57/57=`00 00`，而
  相邻 `+0x1B8..+0x1BB` 磁盘签名在同一语料中明确多值，说明这里不是把整个
  MBR 尾部机械当成固定零。

因此 `+0x1BC..+0x1BD` 共 **2B 部分闭环 -> 完全闭环**。其完成语义是
**标准 MBR 保留 / 无所有者兼容字**：跨已知写入端配置类型的所有权、
负语义消费端和实盘行为已经闭合。完全闭环不意味着未来盘面必须为零；
若遇到未知非零兼容值，edpcli 应保留而不是清洗。

##### LBA0 Aigo L8302 第三配置类型：已定位 Netac 格式的完整 MBR 写入端

继续对第三类引导代码做原始二进制反查后，Aigo L8302 不再是“来源未知的特殊
前缀”。严格原始样本
`disk26_491520000_vid3535_pid2000_disk&ven_aigo&prod_l8302_onlyid1911491440_20260903_120554.bin`
的 `LBA0+0x000..+0x18F` SHA-256 为：

```text
00863071fd5db2f4ef7734d384dc46e07d9c423ed59c69407597590b89aa13ec
```

这400B 在随 CEMS 安装的 **8份独立 Netac/硬件二进制**中逐字节出现，
包括：

- `hardware.dll`（模板 VA `0x1012C9E0`）；
- `Netac_USB_API.dll`（模板 VA `0x1014BA58`）；
- `Netac_USB_API64.dll`；
- `hardware1.dll / hardware1hd.dll`；
- `isoupdate/newusb20.dll`；
- Edp 目录下的对应 Netac 32/64 位库。

关键点不是“搜索到相同字符串”，而是 Windows PE 机器码给出了完整写入端：
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

因此 `+0x000..+0x18F` 400B 在该格式配置类型下由嵌入模板**原样生成**；
分区表修改从 `+0x1BE` 附近开始，不会改动这400B。Aigo 实盘与写入端
模板逐字节相等，已经满足该第三配置类型的原始盘交叉验证。

同时又找到旧版 `UsbMainBSec` 的直接 LBA0 写入端，而不再只知道它被
`BuildSector6` 当底模使用：`CUsbRegsiter::UnRegsiterUsb` 在
`sub_100419B0()` 判定分支中执行
`memcpy(temp, UsbMainBSec, sector_size) -> sub_10013E40 -> WriteSectorData(LBA0,1)`。
这证明旧版模板本身确实属于官方 LBA0 写回材料。

不过这批发现仍**不增加完全闭环**。严格缺口已经缩小为两个配置类型选择
问题：

1. 22份生成参考里的旧版注册盘，仍缺“旧注册/格式化版本为何在最终已注册状态
   保留 UsbMainBSec 引导代码”的历史选择链；当前找到的 `UnRegsiterUsb` 写入端
   不能自动等价成旧注册写入端。
2. Aigo 的 Netac 格式底层写入端已闭合，但当前
   `cemssafeudisklabeltool -> usbtoolBusManage` 主链尚未找到静态调用
   `Format*_NetacAPI` 的上层选择点，不能宣称所有制盘都会先走该路径。

所以 LBA0 `+0x000..+0x1B4` 仍保持部分闭环，但“第三配置类型写入端未知”
这一旧表述已经作废。

##### LBA0 Aigo/Netac 上层链继续收敛：`UsbFormat` 不是 Netac MBR 格式的直接调用点

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

但把 `UsbFormat` 本体继续反汇编后，必须修正“它就是 Netac 格式
上层选择器”的初步假设。其前置 `+0x10` 方法会：

1. 先检查目标盘 `X:\\bin\\windows\\sectorManage.dll`，不存在时再检查
   `X:\\Costom\\sectorManage.dll`；
2. 在需要兼容处理的盘上加载 `X:\\Costom\\usb20dll.dll`；
3. 通过 `IF_OpenDevEx / IF_GET_Dev_Info` 读取设备信息命令 `0x52`；
4. 返回字节为 `0xA2` 时走“无需该预处理”的分支，否则上层才进入
   `CCEMSSafeUsbRegsiter::UsbFormat`。

`UsbFormat` 自身按对象 `+0x04` 的办公/普通分支分别进入
`BackPassWordOffice` / `BackPassWord`，两条路径只动态解析
`IF_OpenDevEx / IF_CloseDev / IF_IIR_Manage` 并完成密码/IIR 兼容处理；
没有解析或调用 `IF_DiskFormat`。紧随其后的 `RegsiterSafeUsb` 也分成
已有 V2 标签的读取/修改/写入与 `SecUsbInterface.dll` 新建路径，
同样未出现 Netac `FormatExA_NetacAPI`。

另一方面，目标盘同套兼容库 `usb20dll.dll` 的导出
`_IF_DiskFormat@0x10003DA0` 已精确恢复：

```text
_IF_DiskFormat
  -> fcn.10003480(open/resolve target)
  -> NewUsb20.dll!FormatExA_NetacAPI(...)
```

也就是说，“CEMS 随盘兼容层确实提供 Netac 格式 API”已经由调用关系证明；
但在当前安装包全部顶层可执行文件/DLL 中继续检查普通导入和
`GetProcAddress("IF_DiskFormat")` 名称引用后，除 `usb20dll.dll`
自身的导出名外仍未发现主制标链调用点。这个**负证据很重要**：
不能把 `WriteLabelImp -> CCEMSSafeUsbRegsiter::UsbFormat` 和
`usb20dll!IF_DiskFormat -> NewUsb20!FormatExA_NetacAPI` 两条链凭名称强行拼接。

因此 Aigo L8302 的 400B 模板写入端仍然成立，但剩余问题进一步精确为：
**哪一个历史升级/量产/格式化入口真正调用 `IF_DiskFormat`，以及它以什么
设备/配置类型条件选择该路径。** 该选择条件仍阻止引导代码主体整体闭合；不过后续
逐字节交叉最终得到30B在三种已知写入端中完全不受该选择影响，现已单独拆出闭合。

##### LBA0 前400B：三类引导代码配置类型的30B逐字节不变量闭合

本轮不再把前400B强制当作一个不可拆分状态，而是直接对已知三种真实写入端
做逐字节交集：

1. 当前 SAFE6：`RegsiterUsb` 对 `+0x000..+0x18F` 显式 `memset(0)`；
2. 旧版：官方 `UsbMainBSec` 模板；
3. Aigo L8302：`Netac_USB_API.dll::sub_10003880` 的嵌入 MBR 模板，
   已由真实 Aigo 原盘逐字节验证。

三类模板前400B共同为0的物理字节只有30B。除连续的
`+0x17B..+0x18F` 21B 外，还有9个离散位置：
`+0x0E1/+0x0E8/+0x101/+0x103/+0x10B/+0x10D/+0x124/+0x143/+0x162`。
这些离散字节也逐条回到16 位代码/字符串边界核实，不再因为“位于大块部分闭环中”
而机械降级。

消费端复核结果：

- 旧版 `UsbMainBSec` 的第三条错误消息
  `"Missing operating system"` 固定在 `+0x163..+0x17A`，因此
  **`+0x17B` 正是该 C 字符串的 NUL 终止符**；既有消息指针低字节
  `+0x1B7=0x63` 会把打印路径指到这条消息；
- 旧版终止符之后的 `+0x17C..+0x18F` 20B 没有代码/数据引用，是模板尾部零填充；
- Aigo/Netac MBR 引导代码会先把完整512B从 `0x7C00` 搬到 `0x0600`
  （`mov cx,0x100; rep movsw`）再跳到 relocated 代码；三条错误消息位于原模板
  `+0x08B/+0x0A3/+0x0C2`，最后一条的 NUL 已在 `+0x0DA`。
  其执行路径没有任何引用落入 `+0x17B..+0x18F`，因此这21B在 Netac 配置类型
  明确只是零填充；
- 当前 SAFE6 不执行这套引导代码，而是直接清零同一区域。
- 旧版的7个代码散点也都有精确指令语义：
  `+0x0E1/+0x0E8/+0x124` 是三个 `mov dl,[bp+0]` 的零位移操作数；
  `+0x101/+0x103/+0x10B` 是三个 `push 0` 的零立即数；
  `+0x10D` 是 `push 0x7C00` 的低字节0。Aigo/Netac 在这些偏移已经是零填充，
  当前 SAFE6同样显式清零；
- 旧版前两条错误消息分别是
  `Invalid partition table@+0x12C..+0x142` 与
  `Error loading operating system@+0x144..+0x161`，所以
  `+0x143/+0x162` 分别是其NUL终止符；Aigo/Netac/当前在两处仍均为0。

实盘证据也补成三配置类型闭环：已提交测试夹具同时覆盖当前全零与
旧版 UsbMainBSec；另新增
`tests/fixtures/protocol_evidence/aigo_l8302_netac_lba0_prefix.hex`
锁定 Aigo L8302/Netac 的真实前400B。三类均满足
`LBA0[0x17B..0x190] == zero[21]`，且上述9个离散偏移也全部为0。回归
`lba0_bootstrap_body_closes_all_three_profile_invariant_zero_bytes`
同时锁定旧版指令操作数、三条错误消息终止符以及三配置类型物理零值。

因此：

- 7个旧版指令零操作数字节：完全闭环；
- `+0x143/+0x162/+0x17B` 三个旧版 MBR 错误消息 NUL 终止符：
  完全闭环；
- `+0x17C..+0x18F` 20B 跨配置类型固定全零引导代码尾部填充：
  完全闭环；
- 其余370B 引导代码代码/数据仍因两套非零模板与历史配置类型选择
  未闭合而保持部分闭环。

这次前400B共 **30B 净新增完全闭环**，LBA0 从112/400更新为
**142 完全闭环 / 370 部分闭环**；配置类型选择阻塞项仍真实存在，但不再拖住
与选择无关的逐字节不变量。

##### LBA0 `+0x1B8..+0x1BB`：Windows MBR 磁盘签名 4B 闭合

这4B此前已经有完整写入端和真实盘变化规律，但因为没有找到 EDP 自身的
值相关业务消费端，按旧的过严口径保留为部分闭环。本轮把“标准 Windows
MBR 字段的系统语义”和“EDP 是否附加解释”拆开后，证据链已经闭合。

写入端侧保持既有结论：

```text
CreateDiskMbr
  -> GetSystemTimePreciseAsFileTime
     (fallback GetSystemTimeAsFileTime)
  -> FILETIME 转 Unix seconds
  -> low32
  -> CREATE_DISK_MBR.Signature
  -> IOCTL_DISK_CREATE_DISK
```

Windows 正式结构定义又给出消费端/字段语义：
`DRIVE_LAYOUT_INFORMATION_MBR.Signature` 就是用于唯一标识 MBR 磁盘的
驱动器签名，并由 `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 通过
`DRIVE_LAYOUT_INFORMATION_EX.Mbr.Signature` 返回。也就是说，这4B的消费端
并不需要是 EDP 私有代码；它首先是 Windows 磁盘布局协议自身拥有的标准字段。

同时，本轮把当前 `CEMSUsbRegsiter.dll` 中唯一的
`IOCTL_DISK_GET_DRIVE_LAYOUT_EX (0x70050)` 调用继续追到
`fcn.10046320@0x100465A9`。调用成功后，代码只做：

```text
buffer + 0x04 : PartitionCount == 1
buffer + 0x00 : PartitionStyle == PARTITION_STYLE_MBR
```

而 `DRIVE_LAYOUT_INFORMATION_EX.Mbr.Signature` 位于该头的
`+0x08`，这条 EDP 路径没有读取它。于是“没有 EDP 消费端”不再是证据缺口，
而是**负向语义消费端证据**：EDP 只关心布局类型和分区数量，不对
Windows 磁盘签名叠加第二层业务含义。

真实盘仍保持既有强验证：严格22份原始参考 22/22 非零、共有19个不同值；
同一 onlyid 的重复备份签名稳定，按小端解释又与历史初始化时间
一致。筛选后的协议测试夹具现再加门禁：每份真实测试夹具的签名必须
非零，并且测试夹具集合至少保留两个不同真实签名，防止未来把该字段误清零
或退化成常量。

因此 `+0x1B8..+0x1BB` 共 **4B 部分闭环 -> 完全闭环**。闭合语义是
**标准 Windows MBR 磁盘签名**：写入端、Windows 标准消费端、
EDP 负语义消费端和真实盘均已齐全。

##### LBA0 `+0x1A0..+0x1A3`：可选 SectorSize 兼容覆盖项 4B 闭合

该 DWORD 过去一直因为 0/512 双态和缺少独立读取端保留为部分闭环。本轮继续从
写入端所有权、全组件访问点和历史配置类型的物理独立性三个方向交叉后，已经
可以把它从引导代码主体中彻底拆出。

写入端 / 生命周期：

- Windows `CEMSUsbRegsiter.dll::BuildSector0/sub_10013F10@0x10013FB6` 在 SAFE1
  分支明确执行 `mov [sector0+0x1A0], m_nSectorSize`；
- 旧版 `UsbMainBSec` 模板该 DWORD 固定为512；
- Aigo/Netac `sub_10003880` 使用的整扇 MBR 模板在相同槽位显式为0；
- 当前 SAFE6 主注册链不调用 `BuildSector0`，只保留预读 LBA0 的该4B；
- Linux `CLabelManage::BuildSector0@diskfile.cpp:625` 使用 `m_nSectorSize` 计算
  分区扇区数量，但不把它序列化到 `+0x1A0`。

消费端侧的负证据也已补齐。旧版 16 位引导代码不读取本槽；对当前
`CEMSUsbRegsiter.dll` 全模块按 `+0x1A0` 数据偏移复核后，唯一真正的扇区数据
访问就是上述 SAFE1 写入端存储。另一个 `push 0x1A0` 已逐指令确认只是
`memset(local_buffer, 0, 0x1A0)` 的长度。`UDiskLabelRepair.dll` 没有盘面
`+0x1A0` 读点，Linux 没有 `ReadSector0` 消费端；`EdpEDiskCtrl.dll` 中看似
`object+0x1A0` 的命中也已抽样追到相邻虚表槽调用，属于 C++ 方法表/对象布局，
不是 LBA0 暂存缓冲区。

历史实盘进一步证明 0/512 不是两个引导代码配置类型，而只是同一引导代码上的
独立覆盖项。对 `nopwd_tool/backup + utils/backup` 当前可用完整历史快照重新聚类，
其中 **47份**具有完全相同的官方旧版 `UsbMainBSec` 前400B：

- 10份 `SectorSize=0`；
- 37份 `SectorSize=512`。

把每份 LBA0 的 `+0x1A0..+0x1A3`、磁盘签名 `+0x1B8..+0x1BB` 和标准
分区表 `+0x1BE..+0x1FD` 三个独立变化区归一化为0后，47/47 整个512B
LBA0 的 SHA-256 都严格等于：

```text
2c8877b90c5d42d73f17c511ef5984efc8135543bda0746ef54f347320d78e8f
```

这直接排除了“SectorSize=0/512 代表不同引导代码主体”的解释。已提交协议
测试夹具又同时包含0与512两种真实状态，新增测试要求两态都持续存在。

因此 `+0x1A0..+0x1A3` 共 **4B 部分闭环 -> 完全闭环**，按
**可选 SAFE1 / 旧版 SectorSize 兼容覆盖项** 建模：512 表示该兼容
元数据被历史 SAFE1/旧版写入端填充；0表示覆盖项缺失/无所有者，当前
SAFE6 只透明保留。完全闭环不意味着未来出现其它值时可以机械归零，未知值仍应
兼容保留并报告。

#### LBA1/LBA2 GPT：一方虚拟写入端正向闭环

此前 GPT 只闭合到“官方结构 + 静态写入端/消费端”，当前 22份物理 SAFE6
参考的 LBA1/LBA2 又全部为零，因此两扇都保守留在部分闭环。本轮不再用结构推测，
而是在不接触任何物理原始设备的 Unicorn x86-64 环境里直接执行 Linux 一方
`libcemsfilesyscheck.so`：

```text
CLabelManage::BuildSector0_Gpt @ 0x1FD30
CLabelManage::BuildSector1_Gpt @ 0x1FDAA
CLabelManage::BuildSector2_Gpt @ 0x1FFF6
```

只替换 ELF 外部的 libc `memset/memcpy` ABI 边界；GPT 构造器和内部
`calculate_crc32@0x1FC49` 原生执行。确定性输入为 2GiB 磁盘、512B 扇区、分区 GUID
`00112233445566778899aabbccddeeff`。`BuildSector2_Gpt` 先写 LBA2 条目0，随后
`BuildSector1_Gpt` 对 `base+2*sector_size` 起完整 `32*512=16KiB` GPT 分区数组
求 CRC，再生成整512B 主头。

一方输出经独立解析得到：

- `EFI PART` / GPT 版本 `0x00010000` / HeaderSize=92；
- `CurrentLBA=1`、`BackupLBA=4194303`、`FirstUsable=34`、`LastUsable=4194270`；
- 磁盘 GUID=`a2a0d0ebe5b9334487c068b6b72699c7`；
- PartitionEntryLBA=2，条目数=128，条目大小=128；
- 独立 IEEE CRC32 命中头 `0xA4B46C72` 与数组 `0xD32CFEA7`；
- LBA1 `+0x5C..+0x1FF` 是官方512B模板自身的零尾，不是测试框架填充；
- 条目0 类型 GUID=`a2a0d0ebe5b9334487c068b6b72699c7`，分区 GUID 为调用方输入，
  `start=63`、`end=4194270`、`attr=0`、`name[72]=0`。

跨平台消费端又独立闭合：把同一34扇官方构造器暂存镜像交给当前
Windows `CEMSUsbRegsiter.dll::IsAllowRegisterCommonLabel/sub_1002AB70` 原生执行，返回
**2 = GPT**。而 Windows `sub_1002B2F0` 与 Linux `AnalyzeGptPartitionTable` 都按128B
步长遍历 GPT 条目，命中支持的类型 GUID 后读取 `start/end/attr`。

这里不降低严格标准：`BuildSector2_Gpt` 一次只写128B 有效条目，当前 `.so` 内没有它的
有效调用方，也没有负责清理其它条目槽的存储；因此一方写入端测试夹具中
entries1..127 的零值仍不能当成“写入端负责全零”。后续继续追解析器与 Windows
单分区构造器后，才把未使用条目的语义从“未知残留”推进到“无所有者残留”，
详见后文“LBA2 未使用 GPT 条目”小节。

最终状态为：

- **LBA1：512B 完全闭环**；
- **LBA2：512B 完全闭环**，其中条目0 是有效条目的一方写入端/消费端，
  条目1..3 则按 TypeGUID=0 的未使用判别项 + 残留负向消费端闭合。

仓库测试夹具 `official_virtual_gpt_lba1.hex` / `official_virtual_gpt_lba2.hex` 与
`official_virtual_gpt_builder_emits_valid_lba1_and_entry0` 回归固定有效条目/头结构与CRC；
它们仍不把测试框架负责未使用条目 zeros 冒充写入端常量。

#### LBA6/LBA9 长 User：最大合法配置类型的一方写入端→读取端闭环

继续追 `UsbWriteParam.m_usbowner/User` 后，短值与长值两条生命周期都已闭合。

短值写入端链不是“固定32B字符串”：

- `sub_10047690` 把请求 User 写到 `UsbWriteParam+0xFC`，容量=`0x9C=156B`；
- 它调用的 `strcpy_s@0x10097F4F` 机器码逐字节复制，遇首个 NUL 立即返回，**不会清
  目标缓冲区剩余容量**；
- `RegsiterUsb@0x1003B616` 随后调用 `sub_100139F0`，把持久 `UsbWriteParam` 再复制到
  未初始化栈局部 `var_3F4`；User仍使用同一 `strcpy_s`，因此 NUL 后尾部继续继承
  写入端未初始化保留底层字节；
- `BuildSector6` 在 `strlen(User)<32` 时固定复制该数组前32B到 LBA6 `+0x50..+0x6F`；
  当前读取端只按 C 字符串消费首NUL前内容。已提交原始样本中真实非零尾字节与
  这条生命周期一致。

长值链又以一方运行时正向执行验证。三套当前写入端机器码完全同构：

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

User 固定数组只有156B，所以最大合法 C 字符串是155B；此时续段恰为128B，
刚好覆盖 `LBA9+0x100..+0x17F`，最后1B为 NUL。隔离 Unicorn 测试框架直接执行当前
Windows `BuildSector6@0x10013FD0`，LBA9 其它区域先填 `0xCC` 作为边界探针：运行后仅
`+0x100..+0x17F` 被完整改写，`+0x000..+0x0FF` 与 `+0x180..` 仍保持 `0xCC`，证明
写所有权边界不是测试框架零初始化造成的假象。

同一盘面镜像又直接交给当前 Windows `ReadSector6/sub_100152A0`。测试框架只跳过
Windows `FS:[0]` SEH/TEB bookkeeping，并替换不影响协议的分配器/C++ 字符串环境边界；
读取端自身的 SAFE6 校验和、滚动 XOR、标记判定和续段复制均原生执行。
运行命中 `0x15552` 长 User 分支，最终在输出 `UsbWriteParam+0xFC` 恢复完整155B User
及 terminating NUL，与写入端输入逐字节一致。

因此：

- **LBA6 `0x050..0x06F` 32B**：短 C 字符串 + 写入端未初始化保留底层字节与长
  标记+28B 前缀两种状态全部闭合，升完全闭环；
- **LBA9 `0x120..0x17F` 96B**：长 User 续段 + 短配置类型原样保留生命周期、
  写入端写边界与官方读取端往返验证全部闭合，升完全闭环；
- `LBA9+0x114..0x11F` 仍与历史 SAPF 尾部/保留底层字节配置类型重叠，旧SAPF 写入端
  未恢复，所以那12B继续部分闭环，不能被长 User 正例顺带升级。

仓库新增 `official_virtual_long_user_lba6.hex` / `official_virtual_long_user_lba9.hex` 和
回归 `official_virtual_long_user_uses_lba6_prefix_and_full_lba9_continuation_slot` 固化最大长度、
标记/前缀/续段与写边界证据；物理 22盘没有长 User 的历史事实继续保留，
不与虚拟一方正向盘面证据混计。

#### LBA2 未使用 GPT 条目：TypeGUID 判别项 + 336B 无所有者残留全部闭合

继续回到当前 Windows `CEMSUsbRegsiter.dll` 的真实 GPT 创建路径后，确认
`WriteNormalULabel` 在大盘分支调用 `sub_10037160(..., partition_count=1)`；该函数构造
`DRIVE_LAYOUT_INFORMATION_EX` 时设置 `PartitionStyle=1 (GPT)`、`PartitionCount=1`，
再通过 `IOCTL_DISK_CREATE_DISK(0x7C058)` 与
`IOCTL_DISK_SET_DRIVE_LAYOUT_EX(0x7C054)` 交给 Windows 磁盘 stack 落盘。

这使条目1..3 的 **类型字段** 不再需要猜测。UEFI 2.10 §5.3.3 对 GPT 条目的
`PartitionTypeGUID@+0x00..0x0F` 有明确盘面语义：16B 全零即未使用条目；当前
Windows 路径又明确只提交1个有效分区。因此 LBA2 首扇区的条目1/2/3 三个
类型 GUID（物理 `+0x080..08F / +0x100..10F / +0x180..18F`）可按 标准定义
未使用判别项闭合。

第一阶段证据按严格边界分层：

- 官方 Linux GPT 虚拟测试夹具中这三个类型 GUID 均为0；
- 本机只读扫描20,538个候选文件找到1份真实 GPT 镜像（Ubuntu 26.04 ISO）；其3个
  已使用条目后至少125个未使用条目的类型 GUID/完整条目均为0；
- 但 Linux 测试夹具的其它残留字节来自测试框架预清，Ubuntu 镜像也不是
  EDP/Windows 一方盘面写入端，所以这一阶段**只**升级三个16B 类型判别项，
  不把 `UniqueGUID/start/end/attr/name` 的零值解释成写入端常量。

随后继续追当前解析器，第二阶段把剩余336B的**消费语义**直接钉死。Windows
`CPartitionType::AnalyzeGptPartitionTable/sub_1002B2F0` 对每个128B 条目的真实控制流是：

1. 先以支持的 GUID 映射对 `entry+0x00..0x0F` 做 TypeGUID 比较；
2. 只有 GUID 匹配后才进入 `+0x20/+0x28/+0x30` 的起点/终点/属性读取与
   `PartitionInfo` 构造；
3. GUID 不匹配时直接进入下一条条目，残留不参与任何值相关判断。

Linux 一方 `CPartitionType::AnalyzeGptPartitionTable@0xFB36` 独立给出同一边界：
`memcmp(type_guid, entry, 16)` 命中后才执行 `mov 0x20(entry)`、`mov 0x28(entry)`，再由
`AnalyzePartitionProperty` 读取 `+0x30`；未命中则只递增 iterator/条目索引。

为了排除“反编译看起来没读，但运行时通过 STL 辅助函数间接读残留”的可能，本轮又做了
Windows 一方动态消费端探测：

- 直接调用官方 `CPartitionType` 构造器 `sub_1002A530(512)`，支持的 GUID 映射
  由原始 DLL 构造；只给 Unicorn 映射 `fs:[0]` SEH 零页，并把 CRT
  `HeapAlloc/HeapFree` 替换为等价分配器/free 边界；
- 输入1个512B 条目扇区，4条条目都设置 `TypeGUID=zero[16]`，其余
  **112B/条目全部故意填 `0xA5`**；
- 原生 `sub_1002B2F0(..., sector_count=1)` 返回0且无异常；内存读取钩子对这512B
  总共只观察到每条条目 TypeGUID 起点的短路比较读取，**336B 残留读取次数=0**；
- 该 `0xA5` 是消费端探测，不是写入端测试夹具，专门用于证明未使用残留
  可以取任意值而不进入业务语义。

所以条目1..3 的 `UniqueGUID/start/end/attr/name` 等336B不应继续建模为
“缺 Windows kernel 写零正例的字段”，而应建模为 **未使用条目无所有者残留**：
TypeGUID=0 已经宣告条目不存在，后续112B/条目没有 EDP/官方解析器语义消费。
这与 LBA4/LBA5 的无所有者保留底层字节采用同一完全闭环口径；实现可以标准新盘为零，
但兼容读取端不得把残留非零误判为隐藏分区或强制依赖其零值。

因此 LBA2 再增加 **336B 完全闭环**，至此 **512/512 完全闭环**。回归
`one_partition_gpt_keeps_entries1_to3_partition_type_guids_unused` 继续锁定三个16B 判别项；
严格总账/完整LBA门禁则防止未来把残留又误退成写入端负责全零或部分闭环。

#### LBA6 GSerial / BeiZhu：按首个 NUL 边界再拆 10B

此前将 `GSerial[16]` / `BeiZhu[16]` 整槽回退部分闭环是必要的，因为短字符串 NUL 后
确有当前保留底层字节与旧版 MBR 底层内容两种物理语义。但这也把**首个 NUL 之前
确定属于 C 字符串的字节**一并低估了。

Windows/Linux 当前 `BuildSector6` 对两槽的物理写法已经是机器码级一致：先清16B
临时缓冲区，再从 `UsbWriteParam` 固定复制输入前15B，最后整16B写盘；读取端从
`+0x1C0/+0x1D0` 均按 C 字符串消费。结合严格原始样本：

- GSerial 16/22 为 `322CA28A\0`，6/22 为 `322CA28A-D7D144`；因此
  `+0x1C0..+0x1C7` 8B 永远是有效字符串前缀，`+0x1C8` 也仍是字符串域：短为NUL、
  长为 `'-'`。历史目录另做20个去重前部统计集，无任何前8B反例，byte8仅出现
  `00/2D`；旧版 MBR 底层内容从短 NUL **之后** 才暴露；
- BeiZhu 20/22 为空、2/22 为 GBK“普通”。因此只有 `+0x1D0` 能跨所有配置类型
  保证仍属于 C 字符串本体：空串时是NUL，“普通”时是首字节 `C6`；从 `+0x1D1`
  开始，空串配置类型已允许进入保留底层字节，不能继续升级。
据此新增 **9B GSerial + 1B BeiZhu = 10B 完全闭环**；其余 GSerial 7B、BeiZhu 15B
继续保留部分闭环。回归
`lba6_gserial_and_beizhu_semantic_prefixes_stop_before_profile_underlay` 固定已提交
配置类型的该边界，而不是把字符串具体值硬编码成未来协议常量。


## 11. 历史 DLL / 配置类型取证目标

> 本节只记录已经核验过的二进制版本、哈希、已确认调用链和仍缺失的历史写入端。
> “目标/候选”不等于协议事实；只有落到机器码/源码/真实盘并通过回归后才能升级主账本。

本节固定用于补齐剩余旧版写入端/配置类型分支所需的公开归档身份，不改变严格协议的
完全闭环字节总数。

### 同时期 2020 组件

#### cemsusbregsiter.dll

- 版本：19.11.4.1
- 架构：32 位 PE
- 归档日期：2020-12-17
- MD5：`783d01f19e998a514834bc5e5f4249ad`
- 公开归档详情页：
  `https://www.ijinshan.com/filerepair/cemsusbregsiter.dll.shtml`

主审计已经使用该二进制恢复 2020 年 `HDSerialInfo` 写入端家族。

#### safeusbregsitercems.dll

- 版本：19.4.4.2
- 架构：32 位 PE
- 大小：158.11KB
- 归档日期：2020-12-16
- MD5：`e516454e5b37da8a853702aca7d4261c`
- 公开归档详情页：
  `https://www.ijinshan.com/filerepair/safeusbregsitercems.dll.shtml`

该二进制现已实际取得并以 SHA-256
`cb700a3fdca69b126264d800657a2e941e8b5e5d5e35501968ed40cb694edc31`
固化到 `audit/protocol/evidence_manifest.tsv`。机器码/字符串审计确认它会加载
`usb20dll`，但当前恢复到的接口只包括
`_IF_OpenDevEx/_IF_IIR_Manage/_IF_CloseDev`；二进制中没有 SAFE6/LLGB/EDPF
字段家族，也没有恢复到 `_IF_DiskFormat` 的解析。因此它是更早设备/IIR
兼容层证据，**不是严格旧版 LBA0-LBA12 的直接写入端**，不得再把它作为
“尚未取得的候选”或用函数名推断 LBA0 配置类型选择器。

#### EdpEDiskCtrl.dll

- 版本：3.6.10.18
- 架构：32 位 PE
- 归档日期：2020-05-19
- MD5：`95a06e0d466ba40a7d5c0e6a409e2114`
- 公开归档详情页：
  `https://www.ijinshan.com/filerepair/edpediskctrl.dll.shtml`

主审计已经把该二进制作为独立的 2020 年运行时读取端使用。

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
该日期是页面明确标注的 “加入 to 该 Dr.Web 病毒 database”（加入 Dr.Web
病毒数据库）日期；同一页面又明确写着 “病毒描述加入: 2024-05-05”
（病毒描述加入时间），并且它列出的落地文件中直接包含
`%TEMP%\\install_2024_05_03_15_33_19.log`。因此这份文件树至多证明“某个被该
病毒名描述的样本/安装包包含一套完整 safeudisklabeltool 配套组件”，**不能证明这些
组件早于 2019-11-21**，也不能用来给其中 DLL 定代。

该网页仍可作为“组件集合存在性定位”，但不再作为“更早历史版本获取定位”。
它没有给出这些 DLL 的逐文件版本、哈希或原始字节，因此不属于
物理/虚拟/静态协议证据，也不会让任何部分闭环字节升级。

下一步取得真正有独立版本/PE 时间/哈希证明、且早于或不同于 19.11.4.1 的完整配套
组件后，必须先同时检查四个指纹：join59 写入端、非零 HSerialCRC 输入赋值、
`UsbOnlyInfo=0`、动态 MBR 模板。只有能把输入赋值一路追到最终 LBA 写入的候选
才进入主账本。

### 为什么必须寻找历史同代完整配套组件

尚未闭环的 LBA0 引导代码正文配置类型选择位于已经恢复出的低层 Netac
格式化导出之上。当前产品链可以到达
`SafeUsbRegsiterCems.dll!GetUsbTegsiterObj` 对象和
`CCEMSSafeUsbRegsiter::UsbFormat` 方法，但历史上究竟由什么条件选择
旧版/Netac MBR 引导代码配置类型尚未闭环。

已取得的 19.4.4.2 `safeusbregsitercems.dll` 已被上述审计排除为直接写入端，
所以缺口不再是“拿到这一个 DLL”，而是**取得与历史写入端同代的完整配套调用链**。
当前 `usb20dll!_IF_DiskFormat -> NewUsb20!FormatExA_NetacAPI` 只证明低层
Netac 格式化器家族；仍需从历史 `WriteLabel/WriteNormalULabel` 或其上游配置类型
选择器连接到该格式化器，才能补足“哪条历史部署调用链选择 Netac”的来源信息。
但真实 LBA0 Netac 盘面已与 `Netac_USB_API.dll::sub_10003880` 内嵌模板逐字节闭环，
因此选择器不再影响 LBA0 字节语义完全闭环判定。


## 12. Phison F2 / LBA3 专项取证

> 本节记录已验证的 Phison 制造生态证据与负边界。
> 它证明制造商家族/F2 生命周期，但**不把 F2 信息暂存页直接等同物理 LBA3**。

本节保留追踪外部制造商自有 LBA3 配置类型时获得的证据，**不改变**严格
LBA0-LBA12 的完全闭环字节总数。

### 已确认样本家族

SHA-256 为
`96614750c61e0ad6b05d19e74848c1679f6318dd21de6faee46c92fb05152142`
的 `MPALL_F1_9000_v372_0B.exe` 公开静态分析记录显示，同一个二进制中存在以下字符串：

- `CBaseController::DoF2`
- `CBaseController::read_write_f2`
- `CBaseController::U3_DoF2`
- `CBaseController::WriteF2Mark`
- `CU32SSBaseContoller::U3_DoF2`
- `CU32SSBaseContoller::WriteF2Mark`
- `F1-F2 MARK`
- `F2 Merged`

后续另一代独立 MPALL：
`mpall_f1_7f00_dl07_v503_0a.exe`
（SHA-256 `2cfd1c3ea9d6bec17d8237f0be79ae96f78fe77a40032987c52d3a30996e29bf`）
仍保留同一 F2 生命周期函数名称家族，包括
`DoF2`、`read_write_f2`、`U3_DoF2`、`WriteF2Mark`、`F2 Merged`。

因此，F2 生命周期是跨代 Phison MP 实现家族，而不是某一个构建版本的残留字符串。
这进一步加强了主协议审计已经使用的写入端家族归因，但仍不能单独识别生成已提交
Kingston LBA3 扇区的精确 PS2307/PS2309 盘面配置类型。

### 负边界：MPALL `F2_MP_21` 是配置元数据，不是 LBA3 盘面证据

历史 MPALL 配置示例分别把 `F2_MP_21` 放在 INI 的
`[Parameter Mark]` 段中，并写成 `Parameter Type=F2_MP_21`。一个 2010 年示例使用
`IC Type=PS2251-32`；另一个 2009 年 PS2231 案例也使用同样的参数类型拼写。
来源：

- https://flashboot.ru/forum/index.php?topic=2549.0
- https://flashboot.ru/forum/index.php?topic=2108.0

因此，MPALL 可执行文件中的 `F2_MP_21` / `F2_MP_23` 等字符串只能证明工具的
配置/配置类型层。它们**不能**证明这些 ASCII 值或数字后缀存在于主机可见 LBA3 扇区。
特别是，在没有实际写入/复制路径进入 F2 缓冲区并与真实设备字节匹配之前，禁止用这些
字符串给 LBA3 `+0x020..+0x027` 赋予语义。

### 上一次本地分析恢复出的机器码事实

v3.72.0B 可执行文件已经在不执行 MP 工具的前提下完成离线获取和检查。

- `CBaseController::WriteF2Mark` 位于约 `0x00581B00`。
- 它把对象自有缓冲区约 `this+0x1C00C` 传入 F2 写入路径。
- 随后发起 `"INFO"` 回读，并把前 512 字节与同一缓冲区比较；不一致时进入显式错误路径。
- `CU32SSBaseContoller::WriteF2Mark` 位于约 `0x00487FA0`。
  该控制器家族使用另一种目标命令包装，并从同一对象自有区域传递
  `0x1C0`（448）字节。
- 因此，不能把共享的 `WriteF2Mark` 名称理解为所有控制器类别都使用同一种
  512 字节传输布局。
- 在不同类的虚方法中恢复出多处对 `this+0x1C00C` 的引用，例如
  `C2273Controller::virtual_308`、`C2267Controller::virtual_308`、
  `C2261Controller::virtual_308`。
- 某一类路径会检查缓冲区前缀是否为 `12 01 00 02`；这**不是**已提交 Kingston
  LBA3 盘面配置类型，其实际前缀为 `00 01 00 00`。因此该分支只能作为有效负面
  区分条件，禁止升级成 LBA3 模板。

### FW/BN 最终标记页是真实 MPALL 输入，不是直接 LBA3 镜像

v3.72 归档包含四个 FW/BN BIN 文件，四者都以一个精确的 512 字节标记页结尾：

| 文件 | 最终页文件偏移 | 页 +0x000 | 页 +0x010 前缀 |
|---|---:|---|---|
| `BN67V1292KM.BIN` | `0x8200` | `this is mp mark\0` | `67 01 00 10 01 29 24 42` |
| `BN67V132M.BIN` | `0x8200` | `this is mp mark\0` | `67 01 00 10 01 32 10 42` |
| `FW67FF01V60424M.BIN` | `0x16200` | `this is mp mark\0` | `67 01 01 10 06 04 24 46` |
| `FW67FF01V61110M.BIN` | `0x1C200` | `this is mp mark\0` | `67 01 01 10 06 11 10 46` |

RTTI/虚表恢复已经把此前只有名称的标记页消费端固定到具体地址：
基础版本=`0x55FC90/0x55F570`、Base30=`0x541290/0x541780`、
C2250=`0x535C40/0x5354A0`、C2260=`0x5333A0/0x532B80`、
C2261=`0x5316B0/0x532B80`。直接机器码确认其生命周期：定位到
`file_size - 0x200`，读取 `0x200` 字节，把前 15 字节与
`"this is mp mark"` 比较，再在控制器/版本兼容性检查中消费相邻字节。
因此 PC 端工具把最终页作为正式 FW/BN **版本标记页**处理。

F2 函数也已经固定到代码地址：
`CBaseController::WriteF2Mark@0x581B00` 通过 F2 厂商写入辅助函数发送
`this+0x1C00C`，回读 `INFO` 响应并比较 `0x200` 字节；
`CU32SSBaseContoller::WriteF2Mark@0x487FA0` 使用不同的 `F3 00 83...`
包装和 `0x1C0` 负载。因此，这些函数加强而不是消除了 F2 信息与主机可见稀疏
LBA3 记录之间的边界。

同时已经加入一个可复现的跨家族不变量。两份独立非零 LBA3 物理配置类型的前 40
字节不同，但 `+0x028..+0x1FF` 尾部完全相同：456 个零字节后跟
`"this is mp mark\0"`，SHA-256 为
`5f88797f7273191052e7a9300316e1a4f0f31563db07110a86fa4e648379198f`。
对四个已经固定的 v3.72 FW/BN 文件，把最终标记页前 16 字节移动到末尾后，都会精确
得到相同的 472 字节尾部；随后 BN 页面与物理扇区匹配 500/512，FW 页面匹配
496/512。离线复现脚本是
`scripts/protocol/audit_lba3_phison_marker_page.py`。

这仍不能建立主机端序列化器：PC 可执行文件中没有恢复出 `496B+16B` 旋转路径，
精确物理动态 8 字节值在本地也只出现在物理采集中。因此，已观察到的 16 字节布局
重排只是结构关系，不能归因成 MPALL 变换。当前恢复的任何 PC 端路径都不会构造
稀疏的 `00 01 00 00 ... +0x020..027 ... marker@+0x1F0` 主机扇区。

简单校验和解释也已独立排除。两个首 DWORD `0x459C7EB5` / `0x22A482A8`
都不是 v3.72 FW/BN 整文件或其标记页子区域的标准 CRC32。对严格物理样本，
`0x459C7EB5` 也不是任一 LBA0-LBA12 扇区、完整 6656 字节镜像、LBA3 置零后的
镜像、device_id、容量、扇区数或 onlyid 的 EDP `crc32_bare`。因此目前没有证据
支持把 `+0x020..+0x023` 解释为简单主机侧或固件文件校验和。

### 现有本地采集元数据无法区分 PS2307 与 PS2309

历史 `a8 82 a4 22 00 20 02 16` 配置类型存在于两份 2026-08-03 连续采集，
两者逐字节一致。其旁挂文件只记录 EDP device_id、摘要/时间和分区事实。
2026-08-27 Kingston 旁挂文件增加 VID/PID/容量，但仍没有 USB 序列号、
USB/SCSI 修订号、控制器、固件、ID_BLK 或 NAND ID。保存的 macOS ioreg
快照同样没有匹配的 Kingston 实例。因此，本地归档无法把任一非零 LBA3 配置类型
确定为 PS2307 或 PS2309；两个公开同身份报告只能继续作为假设，不能当作目标归因。

### 当前严格解释

已提交的 LBA3 证据仍然是：

- EDP 自身原样保留并忽略该扇区。
- 真实设备存在全零配置类型和至少两个非零 `"this is mp mark\0"` 配置类型；
  两个非零配置类型共享一个精确 472B 尾部，但动态前 40B 不同。
- 制造商家族属于 Phison MP/FW；v3.72 最终 FW/BN 标记页经过已观察的 16B 布局重排后，
  可以独立复现该 472B 尾部。
- 精确 `WriteF2Mark` 与 FW/BN 兼容读取端地址已经恢复，但它们**不会**构造主机稀疏记录。
- 构造已提交 `00 01 00 00 ... this is mp mark\0` 配置类型的精确写入端、
  `+0x020..+0x027` 的制造商内部含义，以及控制器固件消费端仍未解决。

因此，LBA3 在 EDP 协议层作为“制造商自有不透明、仅原样保留”的扇区，仍为
512B 完全闭环；以上未解决项继续属于制造商来源问题。

### 下一步具体逆向目标

1. 获取并固定目标年代的 PS2307 MPALL v3.34.07 与 PS2309 MPALL v5.35.35
   家族（以及能取得的配套 FW/BN），两个控制器假设必须分开维护。
2. 在这些代际中搜索稀疏主机记录构造器，而不是继续追已经排除的信息页
   `WriteF2Mark` 路径。
3. 追踪 `F2 Merged` / 控制器固件路径，确定正式 FW/BN 标记页如何变成主机可见的
   `marker@+0x1F0` 制造记录，以及 `+0x020..+0x027` 由什么生成。
4. 在选择 PS2307 或 PS2309 分支前，必须先取得精确物理目标的控制器/FW/ID_BLK
   设备专属证据。
5. 任何恢复出的 Phison 私有子字段语义都必须与 EDP 字节账本分开；它们可以丰富
   制造商来源解释，但不会改变 LBA3 仅原样保留的 EDP 所有权边界。
