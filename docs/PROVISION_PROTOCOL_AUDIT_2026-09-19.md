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

**证据口径更新（2026-09-19）：`nopwd_tool/backup` 中由 edpcli/旧工具执行免密转换后产生的快照只能用于产品回归，禁止作为“原始加密标签如何生成”的证据。其中 Aigo U335 `onlyid=2071754312 @ 12:09:32` 已由 MBR/LBA6/LBA7/LBA12 内容确认是转换后的免密状态，因此当前“原始生成协议”参考集仍使用其余 21 份非转换完整备份，再补入独立 SanDisk 原始加密盘，共 22 份。另一方面，`/Users/zhangyuxi/Desktop/u_disk/analyze/disk_data/no_password_disk4` 是 2026-08-23 从真实 SanDisk Ultra 免密码 U 盘只读采集的原始设备快照，不是 edpcli 自生成/转换盘；它作为独立的第 23 份**真实设备行为/profile 证据**纳入 LBA7 等观测，但不替代 22 份“原始生成参考”去证明加密制盘 writer 语义。已知局部实验态仍按 LBA 单独降权。仓库 `tests/provision_protocol_audit.rs` 对转换盘继续显式排除，并把该真实免密盘的 LBA7 单独放在 `protocol_evidence` 下，不让它进入原始生成参考循环。**

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

### LBA4：必须区分历史 raw-zero gap 与 current SAFE6 full rolling

- `0x00..0x17`：明文 `$$$<onlyid>$$$` 及填充。
- `0x18..0x46`：有效 rolling-XOR 密文区，必须无条件按 16 位 word 解码；密文字节自然等于 0 也不能跳过。
- `0x47..0x1fb`：真实盘存在 raw-zero gap 与 rolling-encrypted-zero 两种物理表示；当前22份生成参考都没有在该437B恢复出非零业务 payload。
- `0x1fc..0x1ff`：尾部 rolling-XOR 锚点，解密后为 `LLGB`。
- 新的“按区段处理”规则对当前 22/22 参考样本全部恢复：
  - `0x39..0x3c == "LLGB"`；
  - `0x1fc..0x1ff == "LLGB"`；
  - `u32@0x18 == onlyid ^ 0x88888888`。
- 旧 `inspect.rs` 的“raw 单字节为 0 就恢复成 0”规则会把真实样本 `onlyid=949028302` 的 `LLGB` 错解为 `\0LGB`；本轮已改为只在整个 `0x47..0x1fb` 未写区全零时保留该区物理零，修复后当前 22/22 参考样本均恢复双 `LLGB` 锚点。

#### LBA4 `0x47..0x1FB`：437B UNKNOWN -> PARTIAL

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

因此这437B已经具备物理边界、current full producer变换范围、reader negative
semantic consumer 和真实双 profile 验证，从 UNKNOWN 降为 **PARTIAL**。
但 full builder 并没有显式把437B清零，只是变换已有 backing；raw-zero 初始
producer/选择条件也仍未知，所以严格禁止升 COMPLETE。

强化门禁 `lba4_short_form_must_be_decoded_by_regions_not_by_zero_bytes`：
committed real fixtures 必须同时覆盖 raw-zero 与 rolling-encrypted-zero 两种物理表示，
并断言两类样本的 semantic gap 都为 zero[437]。

#### current SAFE6 分支纠偏：Provision 现有 short canonical 不是官方 current writer

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

#### LBA4 `+0x45/+0x46`：post-XOR wire flags 已闭合，inspect / Provision 已修正

Linux DWARF 将 restore node 最后2B正式命名为：

- `node+0x2D = bDataToServer`；
- `node+0x2E = bConnetServer`。

Windows 与 Linux producer 现已独立闭合相同的 wire rule：

- Windows `sub_10014550`：完整 rolling loop 后执行
  `LBA4+0x45=node+0x2D`、`LBA4+0x46=node+0x2E`；
- Linux `CLabelManage::BuildSector4@0x1D08E, diskfile.cpp:740`：
  `0x1D278..0x1D2F0` 做完整 0xF4-word rolling，随后
  `0x1D302..0x1D329` 同样 post-XOR 写回这2B。

reader 也已逐指令复核：

- Windows `sub_10015090` 统一 rolling 后复制 0x2F node，只校验
  `OnlyIdXor8`，不恢复两字节；
- Linux `ReadSector4@0x1E048, diskfile.cpp:956` 在
  `0x1E18C..0x1E1D8` 做同一 rolling，再 memcpy 0x2F node，仍只校验
  `OnlyIdXor8`，同样没有补偿 post-XOR store。

所以这里存在一个真实的 producer/reader 非对称，但必须限制在 **current SAFE6
writer profile**：current writer 的物理 raw `+0x45/+0x46` 才是 post-XOR
producer-side flag 值；generic rolling 后得到的是官方 reader 的 transformed
bytes。legacy restore-node profile 不能反向套用这一 current 规则，因为旧 writer
尚未定位。

严格22份原始生成参考重新独立复算（21份非转换 backup + 独立 SanDisk）：

- 22/22 physical bytes 与 generic rolling 输出不相等；
- 6/22 current-style：同时满足
  `OnllyID2Nd==main onlyid && HSerialCRC[5]==0`，physical=`00 00`，generic
  为6组不同非零值；这与当前 Windows `RegsiterUsb` 对 node 整体清零、只赋值到
  `+0x2C`、再由 BuildSector4 post-XOR 写回两个0字节完全吻合；
- 14/22 legacy：`OnllyID2Nd!=main onlyid && HSerialCRC[5]!=0`，physical 非零，
  generic=`00 00`；
- 2/22 legacy（Aigo rev_pmap + SanDisk）：同属 legacy identity profile，
  physical 非零，generic=`0B 00`。

实现已经同步修正：

- `src/inspect.rs`：先 rolling 解码并处理历史 raw-zero gap；仅当 restore node
  满足 current profile（`OnllyID2Nd==main onlyid && HSerialCRC[5]==0`）时，
  再把 `decoded[0x45/0x46]` 恢复为 `raw[0x45/0x46]`。legacy profile 保留
  official ReadSector4 rolling 视图；inspect 继续明确展示
  `bDataToServer / bConnetServer`；
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

因此这2B仍保持 **PARTIAL**：字段名、current producer/wire exception、官方
reader 非对称行为、legacy rolling-reader profile 和22盘分布均已闭合；但 legacy
非零 profile 的旧 producer 以及最终业务 consumer 仍缺失。按严格规则不能因为
“current wire value 已搞清”就升 COMPLETE。

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

current Windows 机器码确实在 node 清零后显式写入前四项固定值：
`SingleUsbFlg=0`、`NewLabFlag=LLGB`、`Version=1`、
`08 04 0C 01`。但是当前 Windows/Linux `ReadSector4` 路径只把整个
0x2F node 复制给调用者并强校验 `OnlyIdXor8`；尚未找到这些常量字段各自的
最终业务 consumer。因此即使它们 22/22 一致，仍不升 COMPLETE。

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

#### Provision LBA4 canonical 修正（历史阶段；已被本轮 SAFE6 full-rolling 证据部分推翻）

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

### LBA6：`0x1C0..0x1EF` 必须拆开

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

旧 profile 必须单独看：两份 legacy 实盘的 post-NUL 字节不是随机残值，
而是与后续 `+0x1E0` 连成一份结构完整的旧 MBR partition table。也就是说
“post-NUL backing bytes”是**profile-dependent**：current 路径不能赋予其业务
字段语义；legacy 两盘却确实保留了 MBR 几何。不能再统一叫“opaque garbage”。

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
  不能再标成“注册标志”。

本轮还重新从 Linux 官方二进制本身核对当前模板，而不是沿用旧文档：
`nm -S -C` 定位 `UsbMainBSec@0x22BB40,size=0x1000`，`.data`
原始字节显示模板相对 `+0x1E0..0x1EF`（VMA `0x22BD20..0x22BD2F`）
确为 16B 零；`BuildSector6` 从该模板起步且不再覆盖这一段。
这只闭合了 **current profile 的零来源**，不能解释两份旧 profile。

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

producer/consumer 边界也重新核过：

- current Windows `sub_10013FD0` 与 Linux `BuildSector6` 都从静态
  `UsbMainBSec` 复制整扇；
- current Windows 在写 GSerial/BeiZhu 时，对临时缓冲先清零，再复制输入槽
  前15B并固定覆盖16B；
- Windows 当前 `UsbMainBSec@0x100E7220` 只发现读取 xref，没有运行时写入；
- Linux `UsbMainBSec@0x22BB40` 同样只在 current builder 被读取；
- current `BuildSector0`/Netac/hardware MBR builder 都只构造单条普通分区，
  不能生成这里的三分区 legacy layout；
- `UDiskLabelRepair.dll` 的 `CLabelRepair::CheckSafe6LabelExist`
  直接从 LBA12 解析 type1/2/4，`Repair0Sector/ReCreate0Sector` 从 sector9/
  backup sector 恢复或重建 LBA0；目前没有发现它直接读取 LBA6 fragment。

因此本轮可以把“legacy opaque extension”这一旧命名**正式撤销**，但仍不能
把 `+0x1E0..0x1EF` 升 COMPLETE：结构语义与两块实盘已经很强，current-zero
producer 也闭合，但生成动态 legacy MBR underlay 的旧 writer 以及直接消费该
LBA6 fragment 的 consumer 仍未找到。

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

### LBA6 `+0x100..0x107`：官方 `m_crcUsbID[2]` 与 doubled guard 已恢复

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
但这里必须区分“运行时成员的用途”与“LBA6 持久化副本的 consumer”：
Linux `ReadSector6` 当前并不读取 `+0x100/+0x104`，所以不能因为同源成员被其它 builder
使用，就把 LBA6 这8B的物理副本直接升 COMPLETE。

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

因此本轮**命名和算法已经闭合，但 COMPLETE 字节数不增加**：

- `+0x100..103`：PARTIAL，原因仅剩“LBA6副本没有活跃 consumer”；
- `+0x104..107`：PARTIAL，已知 doubled guard 的 historical consumer，
  但 current 三个同源实现中的该判断均不可达。

禁止后续仅凭死代码或“同源成员用于其它扇区加密”把这8B升级 COMPLETE。

### LBA8：加密长度由 LLGB +0x04 决定，不是固定 368B

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
  LBA8 严格状态同步纠正为 **86 COMPLETE / 426 PARTIAL / 0 UNKNOWN**。
- LBA8 `+0x14..+0x17` 与 LBA4 解密后的 `0x35..0x38` 在当前 22/22 参考样本逐字节一致，是跨扇区动态字段，不是可固定 profile 常量。

#### LBA8 current UsbOnlyInfo：main onlyid 的十六进制 wire 表达

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

这次**不增加 COMPLETE 字节数**：current UsbOnlyInfo producer 已闭合，
但 legacy `HDSerialInfo` 的生成源以及这 42B 的最终业务 consumer 仍未闭合，
所以 `+0x14..+0x3D` 继续整体保持 PARTIAL。

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
- 在这 22 份“原始生成参考”内部，唯一 LBA7=2 的样本仍是 Netac `onlyid=949028302 @ 17:24:33`；与同 onlyid 的 17:23:49 / 17:24:20 对比，仅 LBA7 发生变化，其余 LBA0–12 一致，因此该扇区继续按局部实验/中间态降权。排除它后，原始 LBA7 参考是 21/21 三条。
- 新纳入的独立真实免密 SanDisk Ultra 则给出**第二个、且是真实在用的两条 entry profile**：entry0=type2、entry1=type4，二者 `NeedDisturb=1`、`NeedEncrypt=1`，`PartionCount=2`。这证明“两条 LBA7”本身不能再被描述成只可能是实验态；它只说明 Netac 那一份不能用于反推原始三分区 writer。
- 对该真实免密盘重新按 packed 0x40 ABI 逐字段解码时，两个 entry 的 `Version@+0x04` 都是 **0**；目录中旧 `disk4_info.json` 的 `"ver": 2` 来自历史解析器把 `PartionCount@+0x08` 错当成 version，现已由回归门禁明确拦截。
- 因此 `+0x08` 必须命名为 `partition_count` / `PartionCount`；表格式代际不能再从该字段推断。

### LBA7 packed 64-byte ABI versus Linux natural 72-byte ABI

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

### LBA7 entry Version / entry1+entry2 NeedDisturb：producer 已前推，consumer 仍未闭合

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

consumer 侧目前得到的是更严格的“只闭合 entry0”结论：

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
- Linux \`libedpedisk.so\` 的 \`PartitionHeader\` 构造函数会携带整条
  \`tagNewEdpPartionInfo\`，但在已扫描的解密/校验路径里没有找到 entry Version
  或 NeedDisturb 的业务分支。

旧版 \`EdpEDiskCtrl.dll\` 也再次表明：旧 LBA7 被读出后，协议代际由
14B pass-info Version 决定/被上层固定为 \`0x64\`，并未发现 entry
\`Version@+0x04\` 用作版本选择。

实盘复核：22份原始生成参考的实际 EDPF entry \`Version@+0x04\` 全部为0；
新增真实免密 SanDisk 的两条 entry 同样为0。但“当前值恒0 + writer 零来源”
仍不能替代一个真实 consumer。

因此本轮不升级任何 COMPLETE 字节：entry Version 与 entry1/entry2
NeedDisturb 继续 PARTIAL。新增
\`lba7_entry_version_is_not_partition_count_across_real_profiles\` 门禁，
专门防止再次把 \`PartionCount@+0x08\` 错读成 Version。

这次跨两版 xref 是**negative consumer evidence**，不是“Reserved”证明：这些字段
在官方 ABI 与 old/new converter 中都会被保留，所以在找到实际策略 consumer 或
历史版本选择语义之前，不能因为当前两个 build 都不读取就升级 COMPLETE。

### LBA7 v0x0064 packed legacy file-key wrapping

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

### LBA7 `+0x0CE..+0x1FF`：306B writer-zero post-table 区完整闭合

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
`183 COMPLETE / 23 PARTIAL / 306 UNKNOWN` 更新为
`489 COMPLETE / 23 PARTIAL / 0 UNKNOWN`。剩余23B为3条 entry Version（12B）、
entry1/entry2 NeedDisturb（8B）与 pass-info `+0x0A/+0x0C/+0x0D`（3B）。

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
| +0x38 | 16 | wrapped file-key material | 部分已知 | mode1/2/3 writer/reader 算法已映射；22份原始盘的44条加密entry全部为mode2，mode1/3缺正向原盘证据，因此仍PARTIAL |
| +0x48 | 16 | extension / alternate key-material slot | 部分已知 | Windows/Linux 96B runtime 的 v0x206 登录/改密只使用 +0x38..+0x47；Linux 检查组件另有 104B natural-aligned ABI，并把对应扩展材料命名为 `EncryptFileKey32[16]`；22盘当前全零但历史/扩展用途仍未闭合 |
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
- 本轮把 `+0x0A` producer 再向上追了一层：当前
  `CUsbRegsiter::CreatePartitions/sub_1003DB50` 先把完整14B pass-info
  `memset(..., 0, 0x0E)`，随后机器码
  `1003E78A..1003E790` 明确执行
  `tail+0x0A = create_arg1+0x109`；而
  `WriteNormalULabel -> sub_10046E80` 又明确执行
  `create_arg1+0x109 = UsbWriteParam+0x7EC`。因此 `+0x0A`
  是注册/制标请求中的显式1B配置输入，不是未初始化噪声或尾部 padding；
- 同一个 current CreatePartitions 首次建表路径对 `tail+0x0C/+0x0D`
  没有任何覆盖写；二者直接继承 14B 全零初始化。结合22份原始参考与新增真实免密
  SanDisk 都为0，可确认当前 writer 的零来源，但仍不能据此把
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 升级 COMPLETE：
  历史非零 producer、单位/取值域和最终 consumer 仍未找到；
- Linux `CDiskReader::ParseSector12` 把完整 14B pass-info 保存到
  `CDiskReader+0x210`。机器码全模块扫描可找到 `Version @+0x210`
  在 `DecryptFileKey` 中的显式读取，却没有找到
  `+0x21A/+0x21C/+0x21D`（分别对应 pass-info
  `+0x0A/+0x0C/+0x0D`）的直接业务读取。这是“解析后保存但当前模块不消费”的负证据；
- 当前 22 份原始参考样本中，`bNoUsbChkPasSafe(+0x0A)` 并非恒零：
  **18/22=0、4/22=1**；且每一份样本的 LBA7/LBA12 取值都逐字节一致。
  因此它明确是会随标签状态变化并跨两份表同步保存的真实字段，绝不能归为 padding；
- `+0x0C/+0x0D` 当前 Windows 主 DLL、另一版 `out_raw_data/EdpEDiskCtrl.dll`、
  Linux `libcemsfilesyscheck.so`，以及本轮补扫的 Linux
  `EdpEDiskQt5/EdpEDiskBack/linuxedpedisk` 客户端路径均未找到直接消费者；
  Linux DWARF 只给出
  `ShareBackuppromptPeriod/EncryptBackuppromptPeriod` 官方字段名。
  当前 22/22 原始参考样本的两字节均为 0；
- 与 `+0x0B bResetFileKey` 类似，“真实样本全零”不能推出 padding。
  `+0x0C/+0x0D` 在找到行为消费者、单位和初始化来源前，只能记为
  **字段边界/官方名称已知，运行时语义未闭合**；
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

所以本轮只增强“为什么**不能**升 COMPLETE”的证据：
`+0x0A` 是显式 producer + 真实可变值 + Init 对外暴露，但缺最终策略 consumer；
`+0x0C/+0x0D` 有官方字段名和 current-zero producer，却仍缺历史非零 producer、
单位/取值域与 consumer。

Linux `PartitionHeader::SetPartitionNewPass` 同时给出负证据：

- 新密码只更新 `UserKeyCRC(+0x30)`；
- 新版 0x206 表只更新 `wrapped key(+0x38..+0x47)`；
- 不修改 `+0x48..+0x57`、`+0x58`、`+0x5c..+0x5f`。

因此这些区域不能解释成“密码修改状态缓存”。

### LBA12 packed Reserved[7]：producer / negative-consumer / real-device 闭合

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

因此 Reserved[7] 可以闭合，但相邻的 packed `+0x48..+0x57`
仍保持 PARTIAL：当前96B runtime 不消费/不更新它，22盘也全零，
但 104B ABI 明确存在扩展 key-material 字段，历史用途尚未闭合。

**LBA12 packed Reserved[7] producer/negative-consumer closure**
已加入回退门禁，防止以后再次把 `+0x48` 扩展槽与
`+0x59` reserved 混成一片“全零 padding”。

按上述严格口径，Windows/Linux 主运行时 96B packed LBA12 当前逐字节进度为：

- **已知 393B / 512B（76.8%）**
  - 三个 entry 中语义闭合字段：49B/entry，共 147B；
  - entry0 `NeedDisturb(+0x10)`：4B，旧兼容 consumer + 22/22 原始盘已闭合；
  - 三个 entry 的 `Reserved[7]`：21B，producer + negative consumer + 66/66 entry 实测闭合；
  - 表尾行为已闭合字段：11B；
  - `0x12e..0x1ff`：210B，写端零初始化且主读端不消费，可定性为 post-table zero padding；
- **部分已知 119B / 512B（23.2%）**
  - 原三条 entry 的 47B/entry 部分已知区中，entry0 NeedDisturb 4B 已移出；
    其余仍包括 Version、entry1/2 NeedDisturb、wrapped key 的 mode1/mode3
    正向样本缺口、扩展材料槽；
  - 表尾剩余 3B：`bNoUsbChkPasSafe/ShareBackuppromptPeriod/EncryptBackuppromptPeriod`，
    字段名已知但完整行为未闭合；
- **未知 0B / 512B（0%）**
  - 当前主运行时格式已经没有“连字段边界/官方名称都不知道”的字节；
  - 但 119B 仍然不能算语义闭合，Provision 不得据此自行生成。

这组数字只描述**主运行时 96B packed 格式**；不把 `libcemsfilesyscheck.so`
的 104B 扩展结构混入统计。

### LBA12 wrapped material 的当前拆分

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

#### mode2 的 `oldSM4` 开关不是新的盘面格式

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

#### EncryptMode=1：EDP A7F0/A6B0 16B wrapping

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

#### EncryptMode=3：标准 AES-128-ECB wrapping + Windows 历史 fallback

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

#### 严格完成状态

22份 original real-device reference set 中：

- 44条需要16B wrapped key 的 type2/type4 entry **全部 EncryptMode=2**；
- mode1 正向样本：**0**；
- mode3 正向样本：**0**。

所以 `+0x38..+0x47` 目前的剩余缺口已经从“算法/分支不明”收缩为：
**mode1/mode3 缺真实正向盘样本**。严格规则要求 producer + consumer +
real-device evidence 三者都存在，因此这16B仍保持 PARTIAL，
完成度数字不增加；但 `oldSM4` 不再作为未解释 wire profile。

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
- 时间窗/次数20B已经重新由官方 producer/consumer 闭合：
  - Linux DWARF `tagEdpEDiskTmpUse@edpdiskglobal.h:481` 给出正式字段名；
  - Windows `CUsbRegsiter::SetTempUse` 真实机器码将开始/结束时间字符串解析为
    两个64位时间值，并把请求 `+0x40` 原样写入 `useCount`；
  - `BusManageImp::WriteNormalULabel` 的机器码在特殊 OutManage-off 模式明确把
    临时使用请求次数写为 `0xFFFFFFFF`，普通模式则从业务请求 `+0x947` 取值；
  - Linux `CheckTempUse` 将 `useCount=0xFFFFFFFF` 当作无限次数哨兵：
    不递减、不回写；0表示次数耗尽；其它正值减1并回写；
  - `ullBTime/ullETime` 与 `time(NULL)` 比较，0表示对应时间边界不启用。
- `reverse[104]` 本轮继续拆到 writer 覆盖边界，而不是因为20/20为零就整体升级：
  - `CUsbRegsiter::SetTempUse` 先对 EETU magic 后的 `0x7C` 字节整体
    `memset(0)`；
  - 随后只执行 `memcpy(EETU+0x18, request+0x44, 0x66)`，即覆盖
    reverse 前102B / LBA9 `+0x18..+0x7D`；
  - reverse 最后2B / LBA9 `+0x7E..+0x7F` 没有任何后续覆盖，因此保留
    明确的 writer 零初始化值；
  - 对 `BusManageImp::WriteNormalULabel/sub_100A99F0` 的真实机器码重新扫描：
    调用 `SetTempUse(&var_BD4)` 前只写 begin/end 字符串和
    `useCount@var_B94`。对应 `tempUse+0x44..+0xA9` 的102B栈区没有初始化写；
    所以前102B即使当前20/20为零，也可能承接 caller/backing bytes，不能升 COMPLETE；
  - Linux `CheckTempUse` 对整个 reverse[104] 无读取，运行时回写只修改 useCount。

因此本轮只把 **LBA9 +0x7E..+0x7F 两字节**升级 COMPLETE：
explicit-zero producer + negative consumer + 20/20 原始 EETU 为零三条证据闭合。
`+0x18..+0x7D` 的102B继续 PARTIAL。
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

但这120B**不能升级 COMPLETE**。原因是 consumer 边界并未完全闭合：

- `modfilesyscheck::ReadMinPassLenInfo` 只输出 magic 与 `+0x04`；
- 但 `EdpDiskCtrl::ReadPassExInfo` 会把完整解密0x80B复制给
  `CEdpDiskControl::GetPassExInfo` 的公开API调用者；
- 当前收集到的组件中未找到外部消费者读取尾部，但也没有找到
  `PEDP_PARTIONPASS_INFO_EX` 的正式结构声明证明这120B是 Reserved；
- 因此“没有搜到调用者”不能冒充 negative consumer。

结论：

- `LBA9 +0x188..+0x1FF` 120B：UNKNOWN -> **PARTIAL**；
- 已闭合 current producer 和 real-device profile；
- 仍缺正式字段名/公开API上层 consumer，禁止升 COMPLETE。
- SAPF 位于 `+0x100..+0x11f`，整段按字节 `^0x88` 还原。其
  `+0x04..+0x13` 是一个完整 16B MBR partition entry，但**不是当前 LBA0 分区项的镜像**：
  当前 14 份 SAPF 样本中 14/14 均与当时 LBA0 `0x1be..0x1cd` 不同。
- `UDiskLabelRepair.dll::CLabelRepair::Repair` 在 LBA0 无效时先检查 LBA9 SAPF；
  `Repair0Sector(from sector 9)` 会把 SAPF 的四个 DWORD 直接写到新 MBR
  `0x1be/0x1c2/0x1c6/0x1ca`，随后写回 sector 0；若该路径失败才尝试尾部备份扇区。
  因此 SAPF 可闭合为 **LBA0 第一分区项的恢复模板/备份项**，不能再描述成“当前 MBR 副本”。

本轮继续把 SAPF 与 LBA9 中间区域按**真实物理边界**拆开，而不是把
`+0x080..+0x17F` 继续整体记 UNKNOWN。

#### current 注册/runtime 对 LBA9 中间256B的写边界

current `CUsbRegsiter::RegsiterUsb/sub_1003B560` 的机器码/反编译控制流明确：

1. 先从 metadata base 一次读取完整13扇 `LBA0..LBA12` 到工作缓冲；
2. 注册过程中显式重建 LBA4、LBA6、LBA8、LBA11；
3. `sub_1003DB50` 只向 `sector_size*7` 与 `sector_size*12` 写入，
   即只重建 LBA7/LBA12；
4. `BakupUsbSec/sub_10040940` 只是把现有缓冲复制到尾部备份位置，
   不修改工作缓冲；
5. 最后把完整13扇工作缓冲写回。

因此 current 注册路径**不拥有也不重建 LBA9**，只会 preserve-existing。
运行时两个独立 setter 又进一步把所有权边界锁死：

- `SetTempUse`：read-modify-write LBA9，但只替换 `+0x000..+0x07F`；
- `SetPassInfoEx`：read-modify-write LBA9，但只替换 `+0x180..+0x1FF`。

对应 getter 也分别只读取首/尾0x80。故 `+0x080..+0x17F` 的256B在
current 注册、临时使用、密码长度路径中均为 preserve-existing 区。

#### `+0x080..+0x0FF`：历史 Dept/backing profile，不是第四个A6B0块

严格参考中有8盘该128B非零，实际每盘只有16或17B非零。尝试以 device-id CRC
把它作为独立0x80 A6B0块解密，结果无任何可识别 magic/结构；而原始字节直接呈现
GBK文本特征。

最关键的跨扇区交叉：

- 4份具有完整76B ELABEL `Dept=` 的真实盘，
  LBA9 `+0x80` 的16B **逐字节等于 ELABEL Dept 的 `dept[60:]`**；
- 另4份 ELABEL Dept 在63B位置截断，并停在 GBK“建”的首字节 `BD`；
  LBA9 `+0x80` 以 `A8` 开头，随后正好是
  `湖输变电运检中心`，与同一完整部门字符串的后续字节吻合；
- 这说明该区至少有一类历史 writer/profile 会把长 Dept 的相邻
  backing/尾段带入 LBA9，而不是独立加密协议块。

旧 producer 的具体 memcpy/对象布局尚未找到，current 组件也没有业务 reader。
因此128B由 UNKNOWN 降为 **PARTIAL**：current preserve/ignore 行为与真实
非零 profile 已闭合，但历史生成源/消费者仍缺。

#### SAPF `+0x114..+0x11F`：32B decode 范围内的 profile-dependent backing

`UDiskLabelRepair::sub_10008550` 固定取 LBA9 `+0x100` 起 **0x20B**，
逐字节 `^0x88` 后再检查 `"SAPF"`。所以 SAPF 的物理解码范围明确是
`+0x100..+0x11F`，不是只到已闭合的 magic+16B恢复项。

14份真实 SAPF 对最后12B `+0x114..+0x11F` 重新解码后至少出现5种形态：

- 有全零 profile；
- 也有 `0xFFFFFFFE`；
- 多组 `0x77xxxxxx` 一类典型32位进程/栈 backing 值；
- 同一硬件/profile 可重复稳定出现同一组尾值。

这些12B不匹配同盘 LBA0 disk signature，也不匹配当前 MBR partition entry。
现有修复行为只闭合到 SAPF magic + 16B恢复项，没有这12B的独立业务消费。
因此它们从 UNKNOWN 降为 **PARTIAL**，并明确标记为
`SAPF decoded trailing/backing bytes`；禁止再按“全零 reserved”处理。

#### SAPF 后 `+0x120..+0x17F`：current preserve/ignore 区

- SAPF reader只解码到 `+0x11F`；
- `vrvaud_c` 的快速检查只取 `+0x100` 的4B magic；
- current `RegsiterUsb` / `SetTempUse` / `SetPassInfoEx` 都不会覆盖
  `+0x120..+0x17F`；
- 14/14 SAPF真实盘及独立SanDisk当前都为零。

但 current writer 的真实语义是 preserve-existing，而不是 fixed-zero producer；
所以这96B同样只从 UNKNOWN 降为 **PARTIAL**，不升 COMPLETE。

新增门禁：

- `lba9_middle_profile_material_must_not_be_canonicalized_to_zero`：
  - committed真实夹具必须继续保留 `+0x80` 的非零 profile；
  - SAPF `+0x114..+0x11F` 必须同时保留 zero 与 nonzero 两类真实反例；
  - committed SAPF 夹具当前 `+0x120..+0x17F` 仍锁定为零观察，
    但文档明确零不是协议要求。

由此 LBA9 账本变为：

```text
54 COMPLETE / 458 PARTIAL / 0 UNKNOWN
```

这次只做 UNKNOWN -> PARTIAL，不增加 COMPLETE。
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
- `+0x28..0x7f`：当前 SanDisk 实盘为零。虽然尚未发现字段消费者和正式字段名，
  但它们已经不能继续标 UNKNOWN：Get/Set 两端都把完整0x80B结构 round-trip，
  所以这88B明确属于 EESI API payload，只是业务语义未解释。

本轮继续专门追 `+0x04`，排除了一个很自然但错误的解释：

- current `CEdpDiskControl::UserLogin/sub_10022F50` 在栈上建立 EESI 输出结构，
  结构基址为 `ebp-0x334`，随后调用 `GetEdpEdiskSetInfo(&var_334)`；
- 因此 `+0x04` 精确对应 `ebp-0x330`，`+0x08` 对应 `var_32C`，
  `+0x18` 对应 `var_31C`；
- 登录函数后续明确读取 `var_32C/var_31C`，并把它们送入 type2/type4
  `SetVolumeLabelA` 路径，但**整个 UserLogin 没有任何 `ebp-0x330` 引用**；
- 所以 `+0x04` 不控制当前登录路径中“是否使用自定义交换区/保密区卷标”；
- `SetEdpEdiskSetInfo/sub_10022920 -> sub_1000FC70` 也只是把调用者完整
  0x80 结构写入，除强制 magic=`EESI` 外不解释/改写 `+0x04`；
- 接口 vtable slot8/slot9 分别暴露 Get/Set EESI，但在当前收集的
  `edpedisk.exe`、`cemsudisk`、`vrvaud_c` 和旧 `edpedisk.exe` 中没有找到
  对这两个槽的外部调用；
- 较旧 `/VRV/edp/EdpEDiskCtrl.dll` 与 SHA 不同的中间版本
  `/VRV/cems/Edp/edpediskctrl.dll` 都没有 `0x49534545(EESI)` 读写路径，
  说明该设置结构属于后续新增功能，不能借旧版行为反推 `+0x04`。

因此 `EESI+0x04` 仍保持 PARTIAL：已知默认值=1、实盘=1、Get/Set 原样透传，
并明确知道当前卷标 consumer **不读取它**；但尚无官方字段名或独立行为 consumer，
不得把它命名为 enable/version/volume-label switch。

本轮进一步把 LBA10 后续区域从“未知”拆成两个不同的 PARTIAL 边界。

#### `+0x28..0x7F`：EESI 内部未解释 round-trip payload

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
没有读取这88B；当前收集到的产品组件也没有外部 vtable Get/Set 调用方。

这足以把88B从 UNKNOWN 降为 **PARTIAL**，但不足以 COMPLETE：
唯一启用 EESI 的 SanDisk 原盘这88B全零，不能替代字段名、非零 profile
或行为 consumer。

#### `+0x80..0x1FF`：不属于 EESI 的 opaque preserved physical tail

两版 setter 都明确采用 read-modify-write：

- 先读取完整512B LBA10；
- 只覆盖前0x80B EESI密文；
- 后0x180B原样保留；
- 再写回完整扇区。

两版 getter 也都只解密/返回前0x80B，从不暴露后384B。
所以后384B不是“EESI padding”，而是**当前 EESI writer 不拥有、
只负责 preserve-existing 的共存物理尾区**。

严格参考中21/22 LBA10整扇为零；唯一启用EESI的独立SanDisk样本
`+0x80..0x1FF` 也全零，测试继续锁定该观察。但 preserve-existing 的实现本身说明：
未来若出现非零历史/其它profile，current setter也会保留它，因此不能根据当前全零
将其升级为 reserved-zero/padding。

因此 LBA10 的严格账本从：

```text
36 COMPLETE / 4 PARTIAL / 472 UNKNOWN
```

调整为：

```text
36 COMPLETE / 476 PARTIAL / 0 UNKNOWN
```

这是“UNKNOWN -> PARTIAL”的证据升级，不增加 COMPLETE。

`UserLogin` 的实际汇编还明确给出对象映射：`+0x08 -> ebp-0x74` 的
`std::string`，`+0x18 -> ebp-0x54` 的 `std::string`；type2/type4
分支分别以这两个对象调用 `SetVolumeLabelA`。另一版
`out_raw_data/EdpEDiskCtrl.dll` 也存在同构路径。因此两个16B字段的最终运行时
用途已经闭合为交换区/保密区卷标，不再只是“文本槽候选”。

### LBA0 legacy MBR message-pointer bytes：+0x1B5..+0x1B7 闭合

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
  SectorSize；22盘有4份为512，其余为0，但尚未找到独立 reader/consumer；
- `+0x1B8..+0x1BB`：已经闭合为标准 MBR disk signature producer：
  `GetSystemTimePreciseAsFileTime`（fallback `GetSystemTimeAsFileTime`）
  -> FILETIME 转 Unix seconds -> low32 -> `CREATE_DISK_MBR.Signature`
  -> `IOCTL_DISK_CREATE_DISK`。22/22 非零、19种值，同一 onlyid 重复备份稳定。
  Windows drive-layout API 会把它作为 MBR Signature 报告，但当前已审 EDP
  `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` 调用均未发现业务逻辑读取该值，因此按本项目
  “EDP consumer 也需闭合”的严格口径继续 PARTIAL；
- `+0x1BC..+0x1BD`：当前官方模板和22盘均为0，但缺独立 consumer，
  仍不因全零而升级。

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
| 0 | 69B | 443B | 0B | 13.5% | 原64B MBR partition table +55AA基础上，legacy MBR 的3个错误消息指针低字节 `+0x1B5..+0x1B7` producer/consumer/22盘 profile 已闭合；其余443B仍PARTIAL |
| 1 | 0B | 512B | 0B | 0% | 官方 BuildSector1_Gpt + GPT_Header(512B) 结构 + Windows `EFI PART` / `header_lba` consumer 已闭合；22/22当前原始SAFE6盘全零，缺正向GPT实盘，因此整扇PARTIAL |
| 2 | 0B | 512B | 0B | 0% | 官方 BuildSector2_Gpt + GPT_Partition(128B) 结构 + Windows 从LBA2起每扇4 entry parser 已闭合；22/22当前原始盘全零，缺正向GPT实盘，因此整扇PARTIAL |
| 3 | 0B | 512B | 0B | 0% | EDP 注册 writer 对整扇 preserve-existing，当前 Windows/Linux EDP reader 不解析；22份原始盘为21零+1 Kingston MP payload，但厂商 producer/固件 consumer 未闭合 |
| 4 | 36B | 476B | 0B | 7.0% | onlyid clear header、OnlyIdXor8、LLGB 双锚点完成；第二 ID/HSerial/profile 字段仍不完整；`+0x047..+0x1FB` 已由 full writer、reader negative consumer 与 raw-zero/rolling-zero 双实盘 profile 从UNKNOWN降PARTIAL |
| 5 | 512B | 0B | 0B | 100% | 两版 EdpDiskCtrl 均只对 LBA5 执行“读整扇→原样写回→检查 ERROR_WRITE_PROTECT(0x13)”；当前注册 writer 读取既有13扇区后不重建 LBA5，因此 preserve existing bytes；22/22原始盘全零 |
| 6 | 4B | 156B | 352B | 0.8% | checksum 4B 完成；GSerial/BeiZhu 的 C-string 语义已知，但固定16B槽跨 current/legacy writer profile 有不同 backing 语义，因此仍 PARTIAL；原所谓“旧 +0x1E0 扩展”已纠正为 legacy MBR partition-table fragment，2/2 非零实盘的 type/start/count 与 LBA12 type4 精确一致，但旧 producer/直接 consumer 尚未闭合 |
| 7 | 489B | 23B | 0B | 95.5% | 已闭合 entry0 NeedDisturb 4B 此前在总账漏记；加回后再计入 `+0x0CE..+0x1FF` 306B writer-zero区，当前只剩3条 entry Version 12B、entry1/2 NeedDisturb 8B、pass-info剩余3B为PARTIAL |
| 8 | 86B | 426B | 0B | 16.8% | LLGB magic + logical length + ElabOffset 完成；ToolVersion、Labversion、writeTime 和 Reserved[64] 已闭合。重新按动态边界审计后，`+0x80..+0x1FF` 不再按样本最大长度切成“正文+UNKNOWN尾巴”：writer只拥有动态加密前缀，之后是 preserve-existing backing，因此整段统一PARTIAL、无UNKNOWN |
| 9 | 54B | 458B | 0B | 10.5% | EETU/EPPE/SAPF三块边界及current preserve范围已拆清：EPPE尾120B为writer-zero但公开API consumer未闭合；+0x080..0x0FF为历史Dept/backing profile，SAPF +0x114..0x11F为profile-dependent backing，+0x120..0x17F为current preserve/ignore，三段均PARTIAL，不再记UNKNOWN |
| 10 | 36B | 476B | 0B | 7.0% | EESI magic + 两个16B卷标槽完成；+0x04仍缺最终业务语义；+0x28..0x7F 已闭合为未解释的 EESI round-trip payload，+0x80..0x1FF 已闭合 current preserve/ignore 边界，二者均因缺字段/历史profile保持PARTIAL，不再记UNKNOWN |
| 11 | 512B | 0B | 0B | 100% | normal register path 使用 `DISK_GEOMETRY_EX.DiskSize`；`UDiskLabelRepair` check/rewrite path 使用 `DISK_GEOMETRY` 的 CHS capacity。两条路径的 producer/consumer 与同盘双 profile 实测均闭合 |
| 12 | 393B | 119B | 0B | 76.8% | 原 372B COMPLETE 基础上，三个 packed entry 的 Reserved[7] 共21B由官方字段名、writer零来源、negative consumer和22盘66/66零值闭合；+0x48扩展槽及其它119B仍PARTIAL |

总计：

- **完成：2191B / 6656B = 32.9%**
- **部分已知：4113B / 6656B = 61.8%**
- **未知：352B / 6656B = 5.3%**

这是一组**严格下限**，故意宁可低估，不把“能生成/能解析”冒充成“已经完全理解”。
后续只有在证据链真正闭合时，字节才能从“未知 → 部分已知 → 完成”升级。

| LBA | 状态 | 当前结论 |
|---|---|---|
| 0 | 部分闭合 | MBR 分区表、55AA、legacy 三个错误消息指针字节已闭合；SectorSize、disk signature 的 EDP-side consumer 与其它 bootstrap/profile 尾部继续追 |
| 1 | GPT profile 部分闭合 | 当前22/22全零；官方 GPT_Header writer/consumer 已知，但缺正向GPT实盘 |
| 2 | GPT profile 部分闭合 | 当前22/22全零；官方 GPT_Partition writer/consumer 已知，但缺正向GPT实盘 |
| 3 | 外部制造区部分闭合 | 21/22 全零，1 份 Kingston MP payload；官方 EDP 注册链原样保留且当前 reader 不解析，厂商生成/消费语义仍未知 |
| 4 | 高度闭合 | 整扇已无UNKNOWN；`+0x047..+0x1FB` 已确认 raw-zero / rolling-encrypted-zero 双历史物理表示且22盘语义均为零。`+0x45/+0x46` 已闭合为 `bDataToServer/bConnetServer` post-XOR wire bytes，并证明官方 ReadSector4 不补偿该例外；inspect 已恢复 producer-side flags，Provision 已改为 current SAFE6 full rolling + post-XOR覆盖。两flag因缺最终业务consumer仍PARTIAL |
| 5 | canonical 已知 | opaque preserve / 写保护探测 scratch；当前 22/22 全零，但零不是协议固定要求 |
| 6 | 部分闭合 | checksum 已锁；GSerial/BeiZhu 的 C-string 语义闭合，物理槽尾为 profile-dependent backing bytes；`0x1e0..0x1ef` 已识别为 legacy MBR entry3/4 fragment，Aigo/SanDisk 2/2 与 LBA12 type4 几何吻合；current模板零来源闭合，但 legacy writer/直接 consumer 仍缺失 |
| 7 | 高度闭合 | 物理0x40 packed ABI、PartionCount、rolling XOR、entry0 NeedDisturb compatibility gate、v0x0064 legacy wrapped8均已锁；`+0x0CE..+0x1FF` 306B post-table区已升级COMPLETE，整扇不再有UNKNOWN；只剩23B PARTIAL：3×Version、entry1/2 NeedDisturb、pass-info +0A/+0C/+0D |
| 8 | 高度闭合 | LLGB/ELABEL + 可变加密长度已锁；ToolVersion/Labversion/writeTime/Reserved 已闭合。严格22盘 `logical_end=0x148..0x183`、encrypted prefix=`0x150..0x190`；Windows/Linux writer 都只覆盖动态前缀，后部 preserve-existing，inspect 也只解前缀并保留非零tail。因此旧102B UNKNOWN已纠正为PARTIAL，LBA8现无UNKNOWN；HDSerialInfo/MacInfo/UsbOnlyInfo 与17-key ELABEL最终consumer继续追 |
| 9 | 高度闭合 | 整扇已无UNKNOWN：EETU首0x80、EPPE末0x80、SAPF 0x100..0x11F及current preserve中间区边界均明确；历史Dept/backing、SAPF尾12B、post-SAPF区与EPPE zero-tail因历史producer/公开API consumer未完全闭合而保持PARTIAL |
| 10 | 高度闭合 | 整扇 current 存储边界已解释：前0x80为 EESI round-trip payload，后0x180为 EESI setter preserve-existing tail；magic/两个16B文本槽已 COMPLETE，+0x04与+0x28..0x7F仍缺具体业务语义/非零profile |
| 11 | 完全闭合 | DRKB/random252/ASCII VID-PID/PDKB 全部已锁；exact DiskSize 与 CHS repair 两种真实 wire profile 的 producer/consumer/实盘均闭合 |
| 12 | 中度闭合 | 主运行时 96B packed layout 已锁，但多个标志/扩展材料/表尾状态仅结构已知；禁止把“entry边界已知”当成“entry语义已知” |

## 尚不能猜测的材料

- LBA4 `+0x45/+0x46` 已闭合 producer/wire 与 ReadSector4 非对称规则；剩余缺口仅是 `bDataToServer/bConnetServer` 的最终业务 consumer，未找到前不得升 COMPLETE；
- EDPF wrapped-key 的 mode1/mode3 正向真实盘样本（算法与consumer已闭合，当前22盘均为mode2）；
- LBA4 onlyID2Nd 及关联动态字段的生成源；
- LBA0 bootstrap 主体/profile 选择、`+0x1A0 SectorSize` consumer，以及 `+0x1B8` disk signature 的 EDP-side consumer；
- LBA6 0x1c0..0x1ed 不同格式代际的准确字段来源。
- LBA7 Version、entry1/entry2 NeedDisturb 与 pass-info `bNoUsbChkPasSafe/BackupPromptPeriod` 的最终消费者；
- LBA12 NeedDisturb 在新版主路径中的进一步业务作用（旧版 fallback 门控已闭合）；
- LBA12 +0x48..+0x57 扩展材料槽在主盘面中的确切用途；
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
- LBA7 物理盘面固定使用0x40 packed entry，不得用 Linux 检查组件的0x48 natural ABI 解析；
- LBA7 v0x0064 默认密码 legacy wrapped8 必须按 `fold32` XOR 解包后通过 FileKeyCRC；
- LBA12 主运行时格式固定为 96B×3，14B 表尾在 `0x120`；
- LBA12 `NeedDisturb` 的真实参考样本分布按 type1={1}、type2={1}、type4={0} 锁定；
  该门禁只表达“当前真实备份观察事实”，不把它升级成协议恒等式；
- LBA12 pass-info `bResetFileKey/ShareBackuppromptPeriod/EncryptBackuppromptPeriod`
  在当前提交真实样本中均为 0；该测试只锁样本事实，不把它们归类为 padding；
- LBA12 `+0x48..+0x57` 当前样本为零，但测试只锁“观察事实”，不把零值升级成已知语义；相邻 `+0x59..+0x5F Reserved[7]` 已由 producer/negative-consumer/66条实盘闭合并独立设门禁。

Phase 1 以后不得绕过这些门禁，也不得把未知区域重新退化为 donor copy。
