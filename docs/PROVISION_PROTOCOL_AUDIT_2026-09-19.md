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

### LBA3：EDP preserve-existing，厂商 MP 语义仍未闭合

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

因此 LBA3 不能继续作为“完全不知道边界”的 UNKNOWN，也不能把尾部 ASCII
误建模成一个独立 EDP 字段。本轮把整扇 512B 调整为 PARTIAL：
**EDP preserve/ignore 边界已闭合，但制造端 producer、字段定义、固件 consumer
缺失，所以 0B 可计 COMPLETE。**

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

这直接否定“`HSerialCRC[5]` 必然是当前 U 盘自身唯一序列”的强解释：

- 已提交 Lexar 与 Netac 真实夹具的 device_id、VID 均不同，但解出的
  `HSerialCRC[5]` 逐字节完全相同且非零；审计测试显式锁住这个反例；
- 22 份全量样本还呈现明显 profile 聚类：
  - 上述 14 份固定 HSerial 组全部同时表现为 LBA9 `EETU+SAPF`；
  - 当前 writer 形态的 6 份 `HSerialCRC=0 && OnllyID2Nd=main onlyid`
    全部同时表现为 LBA9 `EETU+EPPE`；
  - 另外 2 份高熵 HSerial 样本同时是 LBA9 全零、LBA6 扩展区非零。
  这只能记为**格式/注册环境 profile 的相关性**，不能反推因果关系。

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

因此当前不能把 `DeviceNumber/HDSerialCRC` 单 DWORD 与 LBA4
`HSerialCRC[5]` 直接等同；旧 profile 的“输入材料 -> 5×DWORD”转换仍未闭合。

#### Provision LBA4 canonical 修正

本轮发现 Provision 生成器曾把不同 profile 混在一起：

- `+0x18..+0x1F` 被错误当作 8B 随机 `lba4_nonce`；
- `+0x20..+0x33` 被硬编码成 14/22 样本中的旧 profile
  `1D29,7B,4DD,79,7C`；
- `sparse_rolling_encrypt` 还把“明文为 0”错误解释成“物理字节保持 0”，
  与已验证的有效 node 区连续 rolling-XOR 冲突。

现在新盘 canonical 已严格跟随**当前 Windows writer profile**：

- `OnlyIdXor8 = main_onlyid ^ 0x88888888`；
- `OnllyID2Nd = main_onlyid`；
- `HSerialCRC[5] = 0`；
- `0x18..0x46` 整个有效 node 连续 rolling-XOR，即使明文字节为 0 也加密；
- short-form `0x47..0x1FB` 才作为整段“未写区”保持物理零；
- `0x1FC..0x1FF` 使用同一条继续推进的 rolling key schedule 写入 LLGB 锚点。

`ProvisionEntropy.lba4_nonce` 与 `sparse_rolling_encrypt` 已删除；
validator 现在逐字节检查完整 47B current-writer node 和 short-form 物理零区，
防止旧 profile 再次混入新盘生成路径。

### LBA6：`0x1C0..0x1EF` 必须拆开

Windows `sub_10013fd0` 与 Linux `CLabelManage::BuildSector6(UsbWriteParam&, char*)` 两套独立 writer 对齐：

- `0x1C0..0x1CF <- m_usbGSerial[0..14] + NUL`；
- `0x1D0..0x1DF <- BeiZhu[0..14] + NUL`；
- `0x1F0..0x1F3 <- m_encrypt`（低字节布尔值扩成 DWORD）；
- `0x1E0..0x1EF` 当前 writer 没有显式覆盖，只能来自模板或其它版本/宿主后处理。

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
GSerial / 15B BeiZhu。**NUL 后尾字节没有稳定业务语义，可能保留对象尾部残值。**
因此它们不能继续拆成“模板值”“状态值”等伪字段。

全量原始样本当前分布：

- GSerial C 字符串：
  - 16/22 为 `"322CA28A"`；
  - 6/22 为 `"322CA28A-D7D144"`；
  - 在可解析 LLGB 的样本中都与 LBA8 GLab 前缀一致；
- `0x1E0..0x1EF`：20/22 为模板零，2/22（Aigo U335 旧形态 +
  SanDisk 原始盘）存在旧格式非零材料；
- `u32@0x1F0`：22/22 均为 1；官方 writer 字段名是 `m_encrypt`，
  不能再标成“注册标志”。

因此旧免密转换器里的 `0x1CA=128480`、`0x1D4..0x1EC=0`
只能保留为**历史兼容 patch recipe**，不能再进入新盘 Provision 的协议模型。
新盘 canonical profile 现在按当前 writer 边界确定性生成：
`GSerial="322CA28A" + NUL + zero tail`、空 BeiZhu、`0x1E0..0x1EF=0`、
`m_encrypt=1`；不模拟 writer 的未初始化尾字节。

### LBA6 `m_autoid@0x70`：C 字符串闭合，固定槽尾不闭合

Linux DWARF/机器码继续把这条链闭合到字符串语义：

- `UsbLabelParam.m_autoid @ +0x258`；
- `UsbWriteParam.m_autoid @ +0x259`；
- `UsbWriteParam(UsbLabelParam&)` 通过 `strcpy_s(..., 16, ...)` 复制字符串；
- `BuildSector6@diskfile.cpp:672` 固定 `memcpy 16B` 到 LBA6 `0x70..0x7F`；
- `ReadSector6@diskfile.cpp:1005` 再通过 `strcpy_s(...,16,...)` 把
  LBA6 `+0x70` 读回 `UsbLabelParam.m_autoid`；
- `BuildSector8` 把同一 `m_autoid` 序列化为 ELABEL `Autonum=`。

全 22 份原始参考只读复核：

- 22/22 的 LBA6 `+0x70` C 字符串与 LBA8 `Autonum=` 完全一致；
- 分布为 `YD000001` 14、空串 6、`1` 2；
- 但第一个 NUL 之后的固定槽尾经常非零，且不同 profile 呈现不同残留形态。

因此 **m_autoid 的 C 字符串语义已经闭合，但物理 16B 槽没有逐字节完全闭合**。
不能把整个 `0x70..0x7F` 提升 COMPLETE，也不能把 NUL 后内容命名为 padding。
CI 已增加真实夹具门禁：一方面要求 LBA6 C-string == LBA8 Autonum，另一方面
必须保留至少一个“NUL 后非零”的真实反例，防止未来实现把尾部错误归零/语义化。

### LBA8：加密长度由 LLGB +0x04 决定，不是固定 368B

- 当前 22/22 参考样本均满足：
  - 解密后 `0x00..0x03 == "LLGB"`；
  - `u32@+0x04 = 0x80 + strlen(ELABEL)`，**不包含结尾 NUL**；
  - 实际 A6B0/A7F0 长度为
    `((u32@+0x04 / 16) + 1) * 16`，即始终覆盖装有结尾 NUL 的下一块；
  - 该长度之后到 512B 为物理零；
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
- LBA8 `+0x14..+0x17` 与 LBA4 解密后的 `0x35..0x38` 在当前 22/22 参考样本逐字节一致，是跨扇区动态字段，不是可固定 profile 常量。

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

### LBA11：`DRKB + random252`，VID/PID 是 4 字符 ASCII

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
- 但历史 22 份样本仍有 1/22 只能以 CHS 向下取整容量解开：
  Aigo U335 `onlyid=1987718388`。当前 DLL 中确实存在
  `sub_100184C0`，常量 `0x7D8200=255*63*512`，形态与 CHS
  容量换算一致；**当前 build 没有找到它的有效 caller**。
  所以“历史 CHS profile 如何被选择”仍未闭合，不能据此把后半 256B
  在跨版本口径下升级为完成。
- 严格完成统计因此更新为：
  - `0x000..0x0FF`：**256B 完成**；
  - `0x100..0x103`：**4B 完成**（PDKB magic 的 producer 和
    consumer 强校验均闭合）；
  - `0x104..0x1FF`：**252B 部分已知**（当前 profile 的
    `PDKB + UID + zero fill` 已闭合，但历史 CHS profile 的选择条件未闭合）。
- Provision entropy 已同步改成仅接受 `random252`；builder 自己写
  `DRKB`，调用方不再能够把前 4B 协议结构字节当成外部随机材料。

### LBA12：整扇 512B 是一个连续 A6B0/A7F0 密文

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
| +0x10 | 4 | NeedDisturb | 部分已知（entry0兼容行为闭合） | Linux字段名、Windows写端来源；旧版 vrvaud_c 的 NewCheckDisTurbUsb(*) fallback 直接以 entry0 +0x10 非零作为 success 门控；其它 entry/新版主路径的业务作用仍未闭合 |
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
  但尚未找到后续策略分支，仍为部分已知。机器码已确认该复制发生在
  `CEdpEDiskCtrlInterface::Init`：`m_PassInfo+0x0A -> Init输出+0x11`；
  `EdpEDisk.exe` 在初始化时把应用对象 `+0xA4` 作为该输出结构传入，因此该状态会被
  暴露到应用层，但目前没有找到对对应 `app+0xB5` 的直接读取；
- Linux `CDiskReader::ParseSector12` 把完整 14B pass-info 保存到
  `CDiskReader+0x210`。机器码全模块扫描可找到 `Version @+0x210`
  在 `DecryptFileKey` 中的显式读取，却没有找到
  `+0x21A/+0x21C/+0x21D`（分别对应 pass-info
  `+0x0A/+0x0C/+0x0D`）的直接业务读取。这是“解析后保存但当前模块不消费”的负证据；
- 当前 22 份原始参考样本中，`bNoUsbChkPasSafe(+0x0A)` 并非恒零：
  **18/22=0、4/22=1**；且每一份样本的 LBA7/LBA12 取值都逐字节一致。
  因此它明确是会随标签状态变化并跨两份表同步保存的真实字段，绝不能归为 padding；
- `+0x0C/+0x0D` 当前 Windows 主 DLL、另一版 `out_raw_data/EdpEDiskCtrl.dll`
  与 Linux `libcemsfilesyscheck.so` 均未找到直接消费者；Linux DWARF 只给出
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 官方字段名。
  当前 22/22 原始参考样本的两字节均为 0；
- 与 `+0x0B bResetFileKey` 类似，“真实样本全零”不能推出 padding。
  `+0x0C/+0x0D` 在找到行为消费者、单位和初始化来源前，只能记为
  **字段边界/官方名称已知，运行时语义未闭合**；
- `+0x0A` 当前可记为“真实可变状态 + Windows Init 向外暴露”，但
  `bNoUsbChkPasSafe` 这个字段名本身仍不足以证明具体的密码安全绕过策略；
  在找到实际策略分支前不得把它翻译成“跳过安全检查”等确定行为。

Linux `PartitionHeader::SetPartitionNewPass` 同时给出负证据：

- 新密码只更新 `UserKeyCRC(+0x30)`；
- 新版 0x206 表只更新 `wrapped key(+0x38..+0x47)`；
- 不修改 `+0x48..+0x57`、`+0x58`、`+0x5c..+0x5f`。

因此这些区域不能解释成“密码修改状态缓存”。

按上述严格口径，Windows/Linux 主运行时 96B packed LBA12 当前逐字节进度为：

- **已知 372B / 512B（72.7%）**
  - 三个 entry 中语义闭合字段：49B/entry，共 147B；
  - entry0 `NeedDisturb(+0x10)`：4B，旧兼容 consumer + 22/22 原始盘已闭合；
  - 表尾行为已闭合字段：11B；
  - `0x12e..0x1ff`：210B，写端零初始化且主读端不消费，可定性为 post-table zero padding；
- **部分已知 140B / 512B（27.3%）**
  - 原三条 entry 的 47B/entry 部分已知区中，entry0 NeedDisturb 4B 已移出；
    其余仍包括 Version、entry1/2 NeedDisturb、wrapped key 的通用生成关系、
    扩展材料槽、尾部 reserved/padding；
  - 表尾剩余 3B：`bNoUsbChkPasSafe/ShareBackuppromptPeriod/EncryptBackuppromptPeriod`，
    字段名已知但完整行为未闭合；
- **未知 0B / 512B（0%）**
  - 当前主运行时格式已经没有“连字段边界/官方名称都不知道”的字节；
  - 但 140B 仍然不能算语义闭合，Provision 不得据此自行生成。

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
  - `+0x04..+0x0B = ullBTime = 0`；
  - `+0x0C..+0x13 = ullETime = 0`；
  - `+0x14..+0x17 = useCount = FF FF FF FF`；
  - `+0x18..+0x7F reverse[104] = 0`。
- 这20B已经重新由官方 producer/consumer 闭合：
  - Linux DWARF `tagEdpEDiskTmpUse@edpdiskglobal.h:481` 给出正式字段名；
  - Windows `CUsbRegsiter::SetTempUse` 真实机器码将开始/结束时间字符串解析为
    两个64位时间值，并把请求 `+0x40` 原样写入 `useCount`；
  - `BusManageImp::WriteNormalULabel` 的机器码在特殊 OutManage-off 模式明确把
    临时使用请求次数写为 `0xFFFFFFFF`，普通模式则从业务请求 `+0x947` 取值；
  - Linux `CheckTempUse` 将 `useCount=0xFFFFFFFF` 当作无限次数哨兵：
    不递减、不回写；0表示次数耗尽；其它正值减1并回写；
  - `ullBTime/ullETime` 与 `time(NULL)` 比较，0表示对应时间边界不启用。
- `reverse[104]` 虽然20/20为0，但 writer 明确允许从请求复制0x66B数据，
  最终业务 consumer 未闭合，因此仍保持 PARTIAL。
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
- `+0x08..0x17`：16B **Share/type2 卷标**；reader 默认字符串与实盘均为
  GBK“交换区”。`UserLogin` 把该槽赋给本地 `std::string`，在 type2 分支
  直接将其 `c_str()` 传给 `SetVolumeLabelA`；
- `+0x18..0x27`：16B **Encrypt/type4 卷标**；reader 默认字符串与实盘均为
  GBK“保密区”。`UserLogin` 在 type4 分支同样将该槽对应字符串传给
  `SetVolumeLabelA`；
- `+0x28..0x7f`：当前 SanDisk 实盘为零，尚未发现字段消费者，不能据此命名为 padding。

`UserLogin` 的实际汇编还明确给出对象映射：`+0x08 -> ebp-0x74` 的
`std::string`，`+0x18 -> ebp-0x54` 的 `std::string`；type2/type4
分支分别以这两个对象调用 `SetVolumeLabelA`。另一版
`out_raw_data/EdpEDiskCtrl.dll` 也存在同构路径。因此两个16B字段的最终运行时
用途已经闭合为交换区/保密区卷标，不再只是“文本槽候选”。

## 当前逐字节地图状态

### 严格完成口径（2026-09-19）

从本节开始，`完成` 只允许表示“这个字节已经真正搞明白”，标准提高为：

1. 字段/区域边界确定；
2. 写端来源或生成算法确定；
3. 读端消费/行为语义确定；
4. 有真实样本交叉验证；
5. 若存在已知代际/profile 差异，差异本身也必须已经解释清楚。

只满足以下任一条件都**不能**计为完成：

- 22/22 样本相同或全零；
- 只恢复出 DWARF/变量名；
- 只知道加解密算法；
- 只知道 canonical Provision 应该写什么；
- 只知道 writer，但没找到 reader/消费者；
- 只知道 reader，但生成来源仍未知；
- 当前 writer profile 已闭合，但已知旧 profile 的来源仍无法解释。

按这个标准重新逐字节统计 LBA0–12 共 6656B：

| LBA | 完成 | 部分已知 | 未知 | 严格完成率 | 当前计数依据 |
|---:|---:|---:|---:|---:|---|
| 0 | 66B | 446B | 0B | 12.9% | 64B 标准 MBR partition table + 55AA 完成；bootstrap 446B 来源未闭合 |
| 1 | 0B | 512B | 0B | 0% | 官方 BuildSector1_Gpt + GPT_Header(512B) 结构 + Windows `EFI PART` / `header_lba` consumer 已闭合；22/22当前原始SAFE6盘全零，缺正向GPT实盘，因此整扇PARTIAL |
| 2 | 0B | 512B | 0B | 0% | 官方 BuildSector2_Gpt + GPT_Partition(128B) 结构 + Windows 从LBA2起每扇4 entry parser 已闭合；22/22当前原始盘全零，缺正向GPT实盘，因此整扇PARTIAL |
| 3 | 0B | 512B | 0B | 0% | EDP 注册 writer 对整扇 preserve-existing，当前 Windows/Linux EDP reader 不解析；22份原始盘为21零+1 Kingston MP payload，但厂商 producer/固件 consumer 未闭合 |
| 4 | 36B | 39B | 437B | 7.0% | onlyid clear header、OnlyIdXor8、LLGB 双锚点完成；第二 ID/HSerial/profile 字段仍不完整；short/full 扩展区大部分未知 |
| 5 | 512B | 0B | 0B | 100% | 两版 EdpDiskCtrl 均只对 LBA5 执行“读整扇→原样写回→检查 ERROR_WRITE_PROTECT(0x13)”；当前注册 writer 读取既有13扇区后不重建 LBA5，因此 preserve existing bytes；22/22原始盘全零 |
| 6 | 36B | 124B | 352B | 7.0% | GSerial 16B、BeiZhu 16B、checksum 4B 完成；若干固定槽/CRC/flag 仅部分闭合，大量模板区仍未知 |
| 7 | 155B | 51B | 306B | 30.3% | 三个 64B EDPF entry 中 48B/entry 完成，加 11B pass-info；Version/NeedDisturb/key8 等仍部分，表后区域未闭合 |
| 8 | 10B | 400B | 102B | 2.0% | LLGB magic + logical length + ElabOffset(2B)完成；17-key ELABEL 序列化/来源虽已较清楚，但下游语义并未逐字段全部闭合，因此整体只计部分；头部仍有未知区 |
| 9 | 52B | 104B | 356B | 10.2% | EETU magic + ullBTime/ullETime/useCount 共24B完成；SAPF magic+16B MBR恢复项、EPPE magic+最小密码长度完成；EETU reverse及其它空洞仍未闭合 |
| 10 | 36B | 4B | 472B | 7.0% | EESI magic + 两个16B卷标槽完成；+0x04仍缺最终业务语义，其余未闭合 |
| 11 | 260B | 252B | 0B | 50.8% | 前半 DRKB+random252 的 producer/consumer 已双闭合；后半 PDKB magic 4B 也完成；其余当前 DiskSize profile 已闭合，但历史 CHS profile 选择条件仍未解释 |
| 12 | 372B | 140B | 0B | 72.7% | 原 147B entry 完成字段基础上，entry0 NeedDisturb 4B 的 producer/兼容 consumer/22盘实测已闭合；另有11B pass-info与210B post-table padding完成；其余140B仍部分已知 |

总计：

- **完成：1535B / 6656B = 23.1%**
- **部分已知：3096B / 6656B = 46.5%**
- **未知：2025B / 6656B = 30.4%**

这是一组**严格下限**，故意宁可低估，不把“能生成/能解析”冒充成“已经完全理解”。
后续只有在证据链真正闭合时，字节才能从“未知 → 部分已知 → 完成”升级。

| LBA | 状态 | 当前结论 |
|---|---|---|
| 0 | 部分闭合 | MBR 分区表和 55AA 已知；全新盘 bootstrap 来源仍追官方写路径 |
| 1 | GPT profile 部分闭合 | 当前22/22全零；官方 GPT_Header writer/consumer 已知，但缺正向GPT实盘 |
| 2 | GPT profile 部分闭合 | 当前22/22全零；官方 GPT_Partition writer/consumer 已知，但缺正向GPT实盘 |
| 3 | 外部制造区部分闭合 | 21/22 全零，1 份 Kingston MP payload；官方 EDP 注册链原样保留且当前 reader 不解析，厂商生成/消费语义仍未知 |
| 4 | 高度闭合 | onlyid 头、rolling XOR 区、onlyIdXor8、LLGB 双锚点已锁；动态字段生成源继续追 |
| 5 | canonical 已知 | opaque preserve / 写保护探测 scratch；当前 22/22 全零，但零不是协议固定要求 |
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
- LBA12 NeedDisturb 在新版主路径中的进一步业务作用（旧版 fallback 门控已闭合）；
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
- LBA3 唯一 Kingston MP 样本不仅有尾部 `this is mp mark`，还保留
  `+0x001` 与 `+0x020..0x027` 的非零材料，防止以后把整扇误缩成一个字符串字段；
- 同硬件身份存在不同 onlyid；
- LBA12 tail 不是全局常量；
- 全部真实样本的 LBA12 tail 均可由目标 device_id 纯生成；
- canonical GLAB 在真实样本中的一致性。
- LBA4 必须按区段而不是按单字节零值解码；
- LBA8 加密前缀长度由 LLGB +0x04 决定，并额外覆盖保存 ELABEL NUL 的块；
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
