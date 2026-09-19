# 新 U 盘 Provision 协议审计（Phase 0）

日期：2026-09-19
范围：旧审计集合共 23 份前部快照，但其中混入免密/实验态，不能再统称“23 份真实原盘”。当前生成协议参考集改为 22 份只读样本：`nopwd_tool/backup` 中 21 份非免密完整备份 + 1 份独立 SanDisk 原始加密盘；仓库保留 7 份裁剪后的原始 LBA0–12 协议夹具。全程只读，不对物理 raw disk 写入。

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
   - LBA1、2、5：当前 22 份生成协议参考样本全部为全零。
   - LBA3：21 份为全零；唯一非零样本带 Kingston 制造标记 `this is mp mark`，同型号另一真实样本仍为全零。
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

## 逐字节复核新增结论（2026-09-19）

以下结论不是直接采信 `u_disk` 文档，而是先用历史真实前部镜像重新独立复算，再把关键变体裁剪为当前仓库中的 LBA0–12 协议夹具；`u_disk` 只作为候选结论和反编译入口。

**证据口径更新（2026-09-19）：免密转换快照是 edpcli/旧工具自己生成的产品态，只能用于产品回归，禁止作为“原始加密标签如何生成”的证据。旧 23 份集合实际由 `nopwd_tool/backup` 的 22 份 + `no_password_disk4` 的 SanDisk 免密快照组成；其中 Aigo U335 `onlyid=2071754312 @ 12:09:32` 也已由 MBR/LBA6/LBA7/LBA12 内容确认是转换后的免密状态。当前生成协议参考集使用 21 份非免密完整备份，再补入独立 SanDisk 原始加密盘，共 22 份。已知局部实验态按 LBA 单独降权，不把一个被改过的扇区用于推导该扇区原始 writer 规则。仓库 `tests/provision_protocol_audit.rs` 同时显式排除 `_nopwd_` 夹具。**

### LBA4：短版必须按区段解码，不能按单字节 0 特判

- `0x00..0x17`：明文 `$$$<onlyid>$$$` 及填充。
- `0x18..0x46`：有效 rolling-XOR 密文区，必须无条件按 16 位 word 解码；密文字节自然等于 0 也不能跳过。
- `0x47..0x1fb`：前部短版中可能整段未写而保持物理零；完整版中该区存在密文。
- `0x1fc..0x1ff`：尾部 rolling-XOR 锚点，解密后为 `LLGB`。
- 新的“按区段处理”规则对当前 22/22 参考样本全部恢复：
  - `0x39..0x3c == "LLGB"`；
  - `0x1fc..0x1ff == "LLGB"`；
  - `u32@0x18 == onlyid ^ 0x88888888`。
- 旧 `inspect.rs` 的“raw 单字节为 0 就恢复成 0”规则会把真实样本 `onlyid=949028302` 的 `LLGB` 错解为 `\0LGB`；本轮已改为只在整个 `0x47..0x1fb` 未写区全零时保留该区物理零，修复后当前 22/22 参考样本均恢复双 `LLGB` 锚点。

#### LBA4 `0x18..0x46` 官方结构与第二 ID

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

当前 22 份参考样本分三组：

- 6/22：`OnllyID2Nd == main onlyid` 且 `HSerialCRC[5] == 0`，完全吻合当前 Windows writer；
- 14/22：`HSerialCRC[5]` 固定为 `00001D29, 0000007B, 000004DD, 00000079, 0000007C`，跨 Aigo/Lexar/Netac 等厂商复用，但 `OnllyID2Nd` 随标签实例变化；
- 2/22：另一组高熵 `HSerialCRC[5]`，Aigo/SanDisk 之间部分成员重合。

这直接否定“`HSerialCRC[5]` 必然是当前 U 盘自身唯一序列”的强解释。Linux `libbusManage.so::UserInfo::GetHDiskSerialZ()` 已确认会读取注册主机的硬盘序列并保存到 BusManage 的 `DiskInfo` 缓存，因此它是 `HDOnlySerial/HSerialCRC` 的强候选上游；但尚未找到“主机硬盘序列字符串 -> 5×DWORD”的直接转换调用，当前只能记为候选来源，不能写成已闭合公式。

### LBA6：`0x1C0..0x1EF` 必须拆开

Windows `sub_10013fd0` 与 Linux `CLabelManage::BuildSector6(UsbWriteParam&, char*)` 两套独立 writer 对齐：

- `0x1C0..0x1CF <- m_usbGSerial[0..14] + NUL`；
- `0x1D0..0x1DF <- BeiZhu[0..14] + NUL`；
- `0x1F0..0x1F3 <- m_encrypt`（低字节布尔值扩成 DWORD）；
- `0x1E0..0x1EF` 当前 writer 没有显式覆盖，只能来自模板或其它版本/宿主后处理。

真实参考样本也支持这种拆分：多数样本 `0x1E0..0x1EF` 保持模板零，但 Aigo U335 的旧形态在 `0x1E0..0x1EC` 存在非零分区相关材料。因此旧实现把 `0x1D4..0x1ED` 整体称为一个“未知清零区”过粗；新盘 Provision 必须分别建模 `GSerial`、`BeiZhu` 与 `0x1E0..0x1EF` 版本扩展区。

### LBA8：加密长度由 LLGB +0x04 决定，不是固定 368B

- 当前 22/22 参考样本均满足：
  - `raw[0..round_up_16(u32_le(decoded,+0x04))]` 为 A6B0/A7F0 区；
  - 该长度之后到 512B 为物理零；
  - 解密后 `0x00..0x03 == "LLGB"`。
- 真实样本的有效长度覆盖 `0x148 / 0x154 / 0x16b / 0x17a / 0x17c / 0x17e / 0x181 / 0x183` 等多种值，实际加密前缀可为 0x150、0x160、0x170、0x180、0x190。
- 因此现有固定 368B (`0x170`) decoder 会：
  - 对短标签多解无意义块；
  - 对长标签截断真实 `VOL/VOLC` 字段。
- LBA8 `+0x14..+0x17` 与 LBA4 解密后的 `0x35..0x38` 在当前 22/22 参考样本逐字节一致，是跨扇区动态字段，不是可固定 profile 常量。

### LBA11：`DRKB + random252`，VID/PID 是 4 字符 ASCII

- `0x000..0x003 == "DRKB"`：当前 22/22。
- `0x004..0x0ff`：252B 运行时随机材料。
- CRC 输入为：

  `DRKB || random252 || VID_ascii4 || PID_ascii4 || size_le64`

- VID/PID 按备份文件中的四位十六进制 ASCII 文本参与 CRC；将 VID/PID 当作数值 little-endian，当前 22 份参考样本均不能解出 PDKB。
- 后半 `0x100..0x1ff` 用该 CRC 的 4B little-endian 作为 A6B0 key；解密后当前 22/22 均为：

  `PDKB || device_id || 0x00 || zero_padding`

- 当前 21/22 使用物理 DiskSize，1/22 使用 CHS 向下取整容量；唯一 CHS 样本是 Aigo U335 `onlyid=1987718388`。因此读端保留 DiskSize→CHS 双候选是必要兼容行为。
- Provision entropy 应建模为 `random252`，而不是把完整 256B 当作任意随机值；builder 必须自己写入 `DRKB` magic。

### LBA12：整扇 512B 是一个连续 A6B0/A7F0 密文

- 对当前 22/22 参考样本直接执行 `a6b0_full(raw512, CRC32(device_id), counter=0)`：
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
  - `ShareBackuppromptPeriod`、`EncryptBackuppromptPeriod`：官方字段名已知，但当前主 DLL 未找到直接消费点，需要继续追其它组件。
- 已闭合：
  - `bResetFileKey`：Windows `ChangePwd` 的强制改密分支会读取它；置位时重新生成 16B file-key 材料，否则保留并解包原 file-key。
- 当前 22 份参考样本的表尾形态仍只有 4 种；`+0x0B..0x0D` 在该样本集均为 0，
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
- 当前 22 份参考样本：LBA12 为 22/22 三条；LBA7 为 21 份三条、1 份两条。
- 唯一 LBA7=2 的样本是 Netac `onlyid=949028302 @ 17:24:33`；与同 onlyid 的 17:23:49 / 17:24:20 对比，仅 LBA7 发生变化，其余 LBA0–12 一致，因此它被定性为 LBA7 局部实验/中间态。排除该扇区后，原始 LBA7 参考是 21/21 三条。
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
| +0x08 | 4 | PartionCount | 已知 | 写端来源 + 当前22/22均等于实际连续条目数 + Linux字段名 |
| +0x0C | 4 | PartionType | 已知 | 1=Boot / 2=Share / 4=Encrypt；Windows/Linux运行时均消费 |
| +0x10 | 4 | NeedDisturb | 部分已知 | Linux字段名、Windows写端来源、Windows/Linux主运行时负消费证据已知；仍无正向行为消费者 |
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

`NeedDisturb` 目前闭合到“字段名 + 写端来源 + 当前主运行时负消费证据”，
但仍**没有找到正向行为消费者**：

- Windows writer 直接写入 `CreatePartitions(arg2)`；
- 对已逆向的标准三分区创建分支，写端结果已经按 96B entry 基址重新核对：
  - Boot = 1；
  - Share = 1；
  - Encrypt = 0；
- 排除 edpcli 自生成的 `_nopwd_` 备份后，当前真实参考样本全部与该 writer profile 一致：
  type1=1、type2=1、type4=0；
- 这说明“真实参考集 + 当前 Windows writer”目前没有冲突，但仍不能把它升级成
  `PartionType -> NeedDisturb` 的协议恒等式，因为尚未找到真正的运行时消费者；
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
- 因此它可以确认是“真实协议字段 + 已知生成规则”，但**当前运行时行为仍未闭合**。
  当前证据更接近“被保留/透传的兼容字段”，但不能据此升级为“协议全局无效字段”；
  禁止按字段名直接翻译为“扰码开关”“激活”“只读”等具体功能。

### LBA12 pass-info：密码状态组进一步闭合

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
- `+0x0A bNoUsbChkPasSafe` 在当前 Windows 主 DLL 中被复制到对外结构的一个独立字节，
  但尚未找到后续策略分支，仍为部分已知；
- `+0x0C/+0x0D` 当前 Windows 主 DLL 未找到直接消费者；Linux DWARF 只给出
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 字段名。当前 22 份参考 LBA12 样本
  的 `+0x0B..+0x0D` 均为 0，其中 `+0x0B` 已由非样本代码路径证明绝非 padding；
- `+0x0A/+0x0C/+0x0D` 必须继续追初始化和跨组件消费者，
  在闭合前不得归类为“保留零字节”。

Linux `PartitionHeader::SetPartitionNewPass` 同时给出负证据：

- 新密码只更新 `UserKeyCRC(+0x30)`；
- 新版 0x206 表只更新 `wrapped key(+0x38..+0x47)`；
- 不修改 `+0x48..+0x57`、`+0x58`、`+0x5c..+0x5f`。

因此这些区域不能解释成“密码修改状态缓存”。

按上述严格口径，Windows/Linux 主运行时 96B packed LBA12 当前逐字节进度为：

- **已知 368B / 512B（71.9%）**
  - 三个 entry 中语义闭合字段：49B/entry，共 147B；
  - 表尾行为已闭合字段：11B；
  - `0x12e..0x1ff`：210B，写端零初始化且主读端不消费，可定性为 post-table zero padding；
- **部分已知 144B / 512B（28.1%）**
  - 三个 entry 各 47B：Version、NeedDisturb、wrapped key 的通用生成关系、扩展材料槽、尾部 reserved/padding；
  - 表尾剩余 3B：`bNoUsbChkPasSafe/ShareBackuppromptPeriod/EncryptBackuppromptPeriod`，
    字段名已知但完整行为未闭合；
- **未知 0B / 512B（0%）**
  - 当前主运行时格式已经没有“连字段边界/官方名称都不知道”的字节；
  - 但 144B 仍然不能算语义闭合，Provision 不得据此自行生成。

这组数字只描述**主运行时 96B packed 格式**；不把 `libcemsfilesyscheck.so`
的 104B 扩展结构混入统计。

### LBA12 wrapped material 的当前拆分

LBA12 entry `+0x30..+0x47`：

- `+0x30..+0x33`：`CRC32_bare(password)`；默认 `"0000aaaa" -> 0x0429735D`。
- `+0x34..+0x37`：`CRC32_bare(file_key)`。
- `+0x38..+0x47`：16B wrapped file-key material。
- 旧 23 份混合集曾得到“22 份符合默认关系、1 份异常”的统计；由于该集合含免密/实验态，该计数现已撤销，不再作为协议证据。
- `wrapped16 -> file_key -> CRC32(file_key)` 的解包关系本身仍有读端代码闭环；新 22 份参考集的逐样本统计需独立重算后才能重新给出比例。
- 这只能证明当前样本的**解包关系**，不能证明所有版本的 wrapped-key **生成源**。
  尤其旧文档中固定字符串 `LtSWi[2f)j` 的来源存在组件间矛盾，因此不得升级为
  通用 Provision 生成规则。

### LBA9/LBA10 的非零形态

- LBA9 在当前 22 份参考样本中分为：
  - 14 份：EETU + SAPF；
  - 6 份：EETU + EPPE；
  - 2 份：全零。
- 20 份非零 LBA9 的 EETU 解密结果 **20/20 完全一致**：
  - `+0x00..+0x03 = "EETU"`；
  - `+0x14..+0x17 = FF FF FF FF`；
  - 其余 `0x78` 字节均为 0。
  目前只闭合了 magic 与读改写边界，`FFFFFFFF` 的业务语义仍未知，禁止按数值猜成
  “永久”“无效时间”等状态。
- `EPPE` 位于 `0x180..0x1ff`，是独立 128B A6B0 区，counter 从 0 重新开始；当前 6/6 解密为 `EPPE 08 00 00 00` 后零填充。
- Windows `cemsusbregsiter.dll::SetPassInfoEx` 对输入 `+0x04` 明确限制为 6..19，
  构造 `EPPE` 后只覆盖 LBA9 `+0x180..+0x1ff`；Windows
  `edpediskctrl.dll::ReadPassExInfo` 与 `modfilesyscheck.dll::ReadMinPassLenInfo`
  均独立读取同一个 0x80B A6B0 区并检查 `EPPE`。后者把 `+0x04` 返回给调用者，
  因此该 DWORD 可闭合为**最小密码长度**；当前真实样本 6/6 均为 8。
- SAPF 位于 `+0x100..+0x11f`，整段按字节 `^0x88` 还原。其
  `+0x04..+0x13` 是一个完整 16B MBR partition entry，但**不是当前 LBA0 分区项的镜像**：
  当前 14 份 SAPF 样本中 14/14 均与当时 LBA0 `0x1be..0x1cd` 不同。
- `UDiskLabelRepair.dll::CLabelRepair::Repair` 在 LBA0 无效时先检查 LBA9 SAPF；
  `Repair0Sector(from sector 9)` 会把 SAPF 的四个 DWORD 直接写到新 MBR
  `0x1be/0x1c2/0x1c6/0x1ca`，随后写回 sector 0；若该路径失败才尝试尾部备份扇区。
  因此 SAPF 可闭合为 **LBA0 第一分区项的恢复模板/备份项**，不能再描述成“当前 MBR 副本”。
- LBA10 在当前 22 份参考样本中为 21 份全零、1 份 SanDisk EESI；该 EESI 样本仅前 `0x80` 为 A6B0 密文，后 `0x180` 物理零。

#### LBA10 EESI：前 0x80B 的读写边界已闭合

Windows `edpediskctrl.dll` 同时给出读端和写端：

- `GetEdpEdiskSetInfo -> sub_1000f930`：定位 LBA10，读取整扇，但只对前 `0x80` 执行 A6B0 解密并检查 `0x49534545 == "EESI"`；成功后也只向调用者返回这 `0x80`。
- `SetEdpEdiskSetInfo -> sub_1000fc70`：强制写入 `EESI` magic，只加密输入结构前 `0x80`；随后先读取原扇区，只替换前 `0x80`，再把整 512B 写回。
- 因此 `0x80..0x1ff` **不是 EESI 自身的 padding**。当前 writer 明确保留这 384B 原字节；SanDisk 实盘该区恰好全零只能作为样本事实，不能推导协议恒零。

当前唯一 EESI 实盘解密结果：

- `+0x00..0x03 = EESI`；
- `+0x04..0x07 = 1`；reader 在初始化输出结构时也把该 DWORD 默认设为 1，但还没有找到独立消费者足以命名其具体业务语义，因此保持 `EESI +0x04`；
- `+0x08..0x17`：16B 文本槽 A，reader 的默认字符串与实盘均为 GBK“交换区”；
- `+0x18..0x27`：16B 文本槽 B，reader 的默认字符串与实盘均为 GBK“保密区”；
- `+0x28..0x7f`：当前 SanDisk 实盘为零，尚未发现字段消费者，不能据此命名为 padding。

`UserLogin` 在 `GetEdpEdiskSetInfo` 成功后会实际读取两个 16B 文本槽，并在非空时分别交给后续字符串状态设置路径，因此两者不是无意义占位；但其最终 UI/策略目标仍需继续追踪，暂不把字段名扩张成“卷标”等更具体语义。

## 当前逐字节地图状态

| LBA | 状态 | 当前结论 |
|---|---|---|
| 0 | 部分闭合 | MBR 分区表和 55AA 已知；全新盘 bootstrap 来源仍追官方写路径 |
| 1 | canonical 已知 | 当前 22/22 全零 |
| 2 | canonical 已知 | 当前 22/22 全零 |
| 3 | canonical 已知 | 21/22 全零，1 份厂商 mp mark；EDP canonical 可零 |
| 4 | 高度闭合 | onlyid 头、rolling XOR 区、onlyIdXor8、LLGB 双锚点已锁；动态字段生成源继续追 |
| 5 | canonical 已知 | 当前 22/22 全零 |
| 6 | 高度闭合 | SAFE6、device CRC、checksum 已锁；0x1c0..0x1df 当前 writer 来源已拆分，0x1e0..0x1ef 仍存在版本/宿主差异 |
| 7 | 高度闭合 | 64B EDPF entry、PartionCount、rolling XOR 已锁；表尾和 key8 生成源继续追 |
| 8 | 高度闭合 | LLGB/ELABEL + 可变加密长度已锁；动态头字段继续追 |
| 9 | 高度闭合 | EETU/SAPF/EPPE 三块及全零形态已区分 |
| 10 | 高度闭合 | 可选 EESI 前0x80读写边界、magic、两个16B文本槽已闭合；+0x04与+0x28..0x7f业务语义仍待追 |
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
- LBA12 表尾 `+0x0A/+0x0C/+0x0D` 的准确跨组件消费语义。

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
- LBA12 `NeedDisturb` 的真实参考样本分布按 type1={1}、type2={1}、type4={0} 锁定；
  该门禁只表达“当前真实备份观察事实”，不把它升级成协议恒等式；
- LBA12 pass-info `bResetFileKey/ShareBackuppromptPeriod/EncryptBackuppromptPeriod`
  在当前提交真实样本中均为 0；该测试只锁样本事实，不把它们归类为 padding；
- LBA12 `+0x48..+0x57` 与 `+0x59..+0x5f` 当前样本为零，但测试只锁“观察事实”，不把零值升级成已知语义。

Phase 1 以后不得绕过这些门禁，也不得把未知区域重新退化为 donor copy。
