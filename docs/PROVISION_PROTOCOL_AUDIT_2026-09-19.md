# 新 U 盘 Provision 协议审计（Phase 0）

日期：2026-09-19
范围：历史审计使用 23 份真实设备前部快照；仓库当前保留 7 份裁剪后的 LBA0–12 协议夹具。全程只读，不对物理 raw disk 写入。

## 结论

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
   - LBA1、2、5、10：历史 23 份样本均为全零。
   - LBA3：22 份为全零；唯一非零样本带 Kingston 制造标记 this is mp mark，同型号另一真实样本仍为全零。
   - 外部逆向资料库 `/Users/zhangyuxi/Desktop/u_disk` 中存在真实非零 LBA10：前 0x80 经 A6B0 解密后为 `EESI`，后 0x180 物理全零。因此 LBA10 是可选设置扇区，不应继续命名为“保留扇区”。
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

## 逐字节复核新增结论（2026-09-19）

以下结论不是直接采信 `u_disk` 文档，而是先用历史 23 份真实前部镜像重新独立复算，再把关键变体裁剪为当前仓库中的 7 份 LBA0–12 协议夹具；`u_disk` 只作为候选结论和反编译入口。

### LBA4：短版必须按区段解码，不能按单字节 0 特判

- `0x00..0x17`：明文 `$$$<onlyid>$$$` 及填充。
- `0x18..0x46`：有效 rolling-XOR 密文区，必须无条件按 16 位 word 解码；密文字节自然等于 0 也不能跳过。
- `0x47..0x1fb`：前部短版中可能整段未写而保持物理零；完整版中该区存在密文。
- `0x1fc..0x1ff`：尾部 rolling-XOR 锚点，解密后为 `LLGB`。
- 新的“按区段处理”规则对 23/23 样本全部恢复：
  - `0x39..0x3c == "LLGB"`；
  - `0x1fc..0x1ff == "LLGB"`；
  - `u32@0x18 == onlyid ^ 0x88888888`。
- 现有 `inspect.rs` 的“raw 单字节为 0 就恢复成 0”规则会把真实样本 `onlyid=949028302` 的 `LLGB` 错解为 `\0LGB`，必须修正。

### LBA8：加密长度由 LLGB +0x04 决定，不是固定 368B

- 23/23 样本均满足：
  - `raw[0..round_up_16(u32_le(decoded,+0x04))]` 为 A6B0/A7F0 区；
  - 该长度之后到 512B 为物理零；
  - 解密后 `0x00..0x03 == "LLGB"`。
- 真实样本的有效长度覆盖 `0x148 / 0x154 / 0x16b / 0x17a / 0x17c / 0x17e / 0x181 / 0x183` 等多种值，实际加密前缀可为 0x150、0x160、0x170、0x180、0x190。
- 因此现有固定 368B (`0x170`) decoder 会：
  - 对短标签多解无意义块；
  - 对长标签截断真实 `VOL/VOLC` 字段。
- LBA8 `+0x14..+0x17` 与 LBA4 解密后的 `0x35..0x38` 在 23/23 样本逐字节一致，是跨扇区动态字段，不是可固定 profile 常量。

### LBA11：`DRKB + random252`，VID/PID 是 4 字符 ASCII

- `0x000..0x003 == "DRKB"`：23/23。
- `0x004..0x0ff`：252B 运行时随机材料。
- CRC 输入为：

  `DRKB || random252 || VID_ascii4 || PID_ascii4 || size_le64`

- VID/PID 按备份文件中的四位十六进制 ASCII 文本参与 CRC；将 VID/PID 当作数值 little-endian，23 份样本均不能解出 PDKB。
- 后半 `0x100..0x1ff` 用该 CRC 的 4B little-endian 作为 A6B0 key；解密后 23/23 均为：

  `PDKB || device_id || 0x00 || zero_padding`

- 22/23 使用物理 DiskSize，1/23 使用 CHS 向下取整容量，因此读端保留 DiskSize→CHS 双候选是必要兼容行为。
- Provision entropy 应建模为 `random252`，而不是把完整 256B 当作任意随机值；builder 必须自己写入 `DRKB` magic。

### LBA12：整扇 512B 是一个连续 A6B0/A7F0 密文

- 对 23/23 样本直接执行 `a6b0_full(raw512, CRC32(device_id), counter=0)`：
  - `decoded[0..4] == "EDPF"`；
  - `decoded[0x170..0x200] == zero[144]`。
- 因此旧描述“只加密前 368B，后 144B RAW”错误。
- 过去观察到的 `raw[0x170..] == a7f0_full(zero144, key, initial_counter=0x170)`，正是“整扇连续加密”的自然结果，不是独立 tail 格式。

### onlyid：注册时随机 GUID 的 CRC32，不是硬件 ID

- Windows 官方注册链：

  `CoCreateGuid -> GUID raw 16B -> CRC32_bare -> object+0x698 -> LBA4 $$$onlyid$$$`

- 该路径没有把 VID/PID、device_id、容量或 USB serial 混入 onlyid 生成。
- 这与真实样本“相同硬件参数存在不同 onlyid”一致。
- GUID 本身是注册实例随机量；onlyid 只是其 32-bit CRC 压缩结果，不能从 onlyid 唯一恢复原 GUID。
- 跨平台 Provision 不需要依赖 Windows `CoCreateGuid` API，只需要 16B 高质量随机熵并复用相同 CRC32 算法。

### EDPF 14B 表尾：完整字段名已恢复，不是 terminator

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
- 部分已知：
  - `bNoUsbChkPasSafe`：Windows 主 DLL 会把它复制到对外/后续参数结构，但具体策略消费点尚未完全闭合；
  - `bResetFileKey`、`ShareBackuppromptPeriod`、`EncryptBackuppromptPeriod`：官方字段名已知，但当前主 DLL 未找到直接消费点，需要继续追其它组件。
- 历史 23 份 LBA12 样本的尾部形态只有 4 种；`+0x0B..0x0D` 在该样本集均为 0，
  但这只是观察事实，不能解释成协议恒零。

### 官方前部写集：固定 13 sectors

- `RegsiterUsb` 分配/读取缓冲长度为 `sector_size * 0x0d`，并调用：
  - `sub_100136c0(..., count=0x0d)`：从起点读取 13 sectors；
  - `sub_10013810(..., count=0x0d)`：写回 13 sectors。
- `sub_100136c0/sub_10013810` 内部都明确以 `count * sector_size` 计算读写长度，因此 `0x0d` 是数量。
- 从 start LBA=0 开始，13 sectors 正好覆盖 **LBA0..LBA12**。
- `u_disk` 的官方 DLL 虚拟注册 trace 同样记录 `start=0, count=13`。
- 当前产品契约因此统一为：备份、inspect、生成、写入、读回、恢复全部只处理 LBA0–12，共 6656B。

### EDPF：+0x08 是 PartionCount，不是 version

- LBA7（0x40 stride）和 LBA12（0x60 stride）均逐样本验证：`u32@entry0+0x08 == 实际连续 EDPF entry 数量`。
- 23 份样本分布：
  - 20 份：LBA7=3 / LBA12=3；
  - 2 份：LBA7=2 / LBA12=2；
  - 1 份历史中间态：LBA7=2 / LBA12=3。
- 因此 `+0x08` 必须命名为 `partition_count` / `PartionCount`；表格式代际不能再从该字段推断。

### LBA12：主运行时盘面是 96B packed entry；不要与 104B 检查结构混用

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

### LBA12 96B packed entry：当前字段置信度

| 偏移 | 长度 | 当前名称 | 当前置信度 | 证据/限制 |
|---|---:|---|---|---|
| +0x00 | 4 | Flag = `EDPF` | 已知 | 写端固定写入；读端判 magic |
| +0x04 | 4 | Version/entry-local field | 部分已知 | Windows writer/样本均多为0；实际消费语义未闭合 |
| +0x08 | 4 | PartionCount | 已知 | 写端来源 + 实盘条目数 23/23 + Linux字段名 |
| +0x0C | 4 | PartionType | 已知 | 1=Boot / 2=Share / 4=Encrypt；Windows/Linux运行时均消费 |
| +0x10 | 4 | NeedDisturb | 部分已知 | Linux字段名、Windows写端来源已知；主运行时行为仍未闭合 |
| +0x14 | 4 | NeedEncrypt | 已知 | Windows InitDiskInfo/UserLogin 实际消费；0=unencrypted，1=启用透明加密 |
| +0x18 | 8 | StartSector | 已知 | 写端计算、挂载端使用 |
| +0x20 | 8 | SectorSize | 已知 | 实盘=512；布局/挂载使用 |
| +0x28 | 8 | PartionSize | 已知 | 写端计算、UserLogin/mount 参数实际消费 |
| +0x30 | 4 | UserKeyCRC | 已知 | 密码校验链消费；默认密码CRC已复算 |
| +0x34 | 4 | FileKeyCRC | 已知 | 解 wrapped key 后 CRC 校验；Windows UserLogin 明确比较 |
| +0x38 | 16 | wrapped file-key material | 部分已知 | 读端明确消费；默认样本可独立解包闭合，但通用生成源/非默认分支仍未闭合 |
| +0x48 | 16 | extension / alternate key-material slot | 部分已知 | Windows writer 当前零初始化且样本全零；扩展 Linux 104B 结构有同类 `EncryptFileKey32`，但 packed 对应关系与用途未完全证明 |
| +0x58 | 1 | EncryptMode | 已知 | Windows writer/reader + Linux挂载分派；见下方支持矩阵 |
| +0x59 | 7 | padding/reserved | 部分已知 | 当前 writer 零初始化、样本全零；未证明所有版本都必须为零 |

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

`NeedDisturb` 目前只闭合到“字段名 + 写端来源”：

- Windows writer 直接写入 `CreatePartitions(arg2)`；
- 当前注册主调用路径传 1，但历史盘样本同时存在 0/1；
- 尚未找到足够可靠的主运行时消费路径，禁止把它解释成“激活”“只读”或其它具体行为。

按上述严格口径，Windows/Linux 主运行时 96B packed LBA12 当前逐字节进度为：

- **已知 367B / 512B（71.7%）**
  - 三个 entry 中语义闭合字段：49B/entry，共 147B；
  - 表尾行为已闭合字段：10B；
  - `0x12e..0x1ff`：210B，写端零初始化且主读端不消费，可定性为 post-table zero padding；
- **部分已知 145B / 512B（28.3%）**
  - 三个 entry 各 47B：Version、NeedDisturb、wrapped key 的通用生成关系、扩展材料槽、尾部 reserved/padding；
  - 表尾剩余 4B：`bNoUsbChkPasSafe/bResetFileKey/ShareBackuppromptPeriod/EncryptBackuppromptPeriod`，
    字段名已知但完整行为未闭合；
- **未知 0B / 512B（0%）**
  - 当前主运行时格式已经没有“连字段边界/官方名称都不知道”的字节；
  - 但 145B 仍然不能算语义闭合，Provision 不得据此自行生成。

这组数字只描述**主运行时 96B packed 格式**；不把 `libcemsfilesyscheck.so`
的 104B 扩展结构混入统计。

### LBA12 wrapped material 的当前拆分

LBA12 entry `+0x30..+0x47`：

- `+0x30..+0x33`：`CRC32_bare(password)`；默认 `"0000aaaa" -> 0x0429735D`。
- `+0x34..+0x37`：`CRC32_bare(file_key)`。
- `+0x38..+0x47`：16B wrapped file-key material。
- 对默认样本，用独立 SM4 实现复算 `wrapped16 -> file_key -> CRC32(file_key)`，
  历史 23 份中 22 份可在观察到的默认关系下闭合；唯一异常样本同时存在不同
  UserKeyCRC/material。
- 这只能证明当前样本的**解包关系**，不能证明所有版本的 wrapped-key **生成源**。
  尤其旧文档中固定字符串 `LtSWi[2f)j` 的来源存在组件间矛盾，因此不得升级为
  通用 Provision 生成规则。

### LBA9/LBA10 的非零形态

- LBA9 在当前 23 份样本中分为：
  - 14 份：EETU + SAPF；
  - 7 份：EETU + EPPE；
  - 2 份：全零。
- `EPPE` 位于 `0x180..0x1ff`，是独立 128B A6B0 区，counter 从 0 重新开始；7/7 解密为 `EPPE 08 00 00 00` 后零填充。
- LBA10 在当前 23 份提交样本中全零，但 `u_disk` 的真实 SanDisk 样本存在 EESI：仅前 `0x80` A6B0，后 `0x180` 物理零。

## 当前逐字节地图状态

| LBA | 状态 | 当前结论 |
|---|---|---|
| 0 | 部分闭合 | MBR 分区表和 55AA 已知；全新盘 bootstrap 来源仍追官方写路径 |
| 1 | canonical 已知 | 当前 23/23 全零 |
| 2 | canonical 已知 | 当前 23/23 全零 |
| 3 | canonical 已知 | 22/23 全零，1 份厂商 mp mark；EDP canonical 可零 |
| 4 | 高度闭合 | onlyid 头、rolling XOR 区、onlyIdXor8、LLGB 双锚点已锁；动态字段生成源继续追 |
| 5 | canonical 已知 | 当前 23/23 全零 |
| 6 | 高度闭合 | SAFE6、device CRC、checksum 已锁；0x1c0..0x1ed 存在格式代际差异 |
| 7 | 高度闭合 | 64B EDPF entry、PartionCount、rolling XOR 已锁；表尾和 key8 生成源继续追 |
| 8 | 高度闭合 | LLGB/ELABEL + 可变加密长度已锁；动态头字段继续追 |
| 9 | 高度闭合 | EETU/SAPF/EPPE 三块及全零形态已区分 |
| 10 | 部分闭合 | 可选 EESI 已确认；canonical nopwd 可零 |
| 11 | 高度闭合 | DRKB/random252/ASCII VID-PID/size/PDKB 链已锁 |
| 12 | 中度闭合 | 主运行时 96B packed layout 已锁，但多个标志/扩展材料/表尾状态仅结构已知；禁止把“entry边界已知”当成“entry语义已知” |

## 尚不能猜测的材料

- LBA4 中除 onlyid、已知 LLGB 常量之外的生成期动态字节；
- EDPF wrapped-key 在非默认密码/不同 algo 分支下的完整生成关系；
- LBA4 onlyID2Nd 及关联动态字段的生成源；
- LBA0 全新盘 bootstrap 的官方来源；
- LBA6 0x1c0..0x1ed 不同格式代际的准确字段来源。
- LBA12 NeedDisturb 的真实运行时行为；
- LBA12 +0x48..+0x57 扩展材料槽在主盘面中的确切用途；
- LBA12 +0x59..+0x5f 是否仅为所有版本共同 padding；
- LBA12 表尾 +0x02/+0x05/+0x0a 等状态字节的准确语义。

这些内容不得从当前插入 donor 盘复制，也不得以全零替代。实现中把它们显式建模为
ProvisionEntropy / ProvisionProfile 材料；纯 builder 只消费已经验证的输入。
其中 LBA11 仅 `random252` 作为每次 provision 的熵输入，前 4B `DRKB` 是协议结构字节。若后续逆向得到确定生成公式，
再用新的金标测试替换显式材料。

## Phase 0 门禁

tests/provision_protocol_audit.rs 固化以下事实：

- canonical reserved sectors 的真实样本证据；
- 同硬件身份存在不同 onlyid；
- LBA12 tail 不是全局常量；
- 全部真实样本的 LBA12 tail 均可由目标 device_id 纯生成；
- canonical GLAB 在真实样本中的一致性。
- LBA4 必须按区段而不是按单字节零值解码；
- LBA8 加密前缀长度由 LLGB +0x04 向 16B 对齐决定；
- LBA11 固定 DRKB magic、ASCII VID/PID CRC 输入和 PDKB 明文结构；
- LBA12 是完整 512B 连续密文，解密后 144B tail 为零；
- LBA7/LBA12 `+0x08` 等于连续 EDPF 条目数。
- LBA12 主运行时格式固定为 96B×3，14B 表尾在 `0x120`；
- LBA12 `+0x48..+0x57` 与 `+0x59..+0x5f` 当前样本为零，但测试只锁“观察事实”，不把零值升级成已知语义。

Phase 1 以后不得绕过这些门禁，也不得把未知区域重新退化为 donor copy。
