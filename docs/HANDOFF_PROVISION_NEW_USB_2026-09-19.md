# 新 U 盘 Provisioning 交接（2026-09-19）

仓库：`Evolution404/edpcli`

分支：`feat/provision-new-usb-20260919`

基线：已正式发布的 `v2.2.0`，`main@14557e7e54e355c5853479a7220e8ff1f0e5e9e7`。

本轮开工基线为分支 `07f58546caf40672d9358feddbf2c80f024747ac`；本交接内容会随
本轮 LBA4 修正一起推送。接手时必须以该分支最新 `origin` HEAD 为准，禁止退回
`07f5854` 或更早协议基线。

逐字节逆向的长期主账本：
`docs/PROTOCOL_BYTE_TRACE_2026-09-19.md`。

后续所有“完成率”必须以主账本的严格口径为准：只有**字段边界 + 官方
producer + 官方 consumer/行为 + 原始实盘验证**同时闭合，才能标记
`COMPLETE`；仅样本一致/全零、知道字段名、能解密、能生成、只有 writer
或只有 reader 都不得升级。CI 的
`tests/protocol_documentation_contract.rs` 会拦截完成字节回退，以及
`COMPLETE` 行缺 producer / consumer / 实盘证据。

官方制盘主链已经确认：

`cemssafeudisklabeltool.exe -> usbtoolBusManage.dll::CreateBusManageImp
-> BusManageImp::WriteNormalULabel -> CEMSUsbRegsiter.dll::CUsbRegsiter::RegsiterUsb
-> BuildSector* -> WriteSectorData(count=0x0D)`。

截至本次 LBA9 EPPE writer-owned zero tail 闭环，严格统计为：

- **COMPLETE：3743 / 6656B = 56.2%**
- **PARTIAL：2913 / 6656B = 43.8%**
- **UNKNOWN：0 / 6656B = 0.0%**

当前各 LBA 严格状态以主账本为唯一准绳，最新关键增量：

- **LBA4 = 36 COMPLETE / 476 PARTIAL / 0 UNKNOWN = 7.0%**。
  `+0x047..+0x1FB` 的437B已经从 UNKNOWN 降到 PARTIAL：严格22份原始生成参考中
  18份为 physical raw-zero gap，4份为 rolling-encrypted 形态；4/4 rolling 形态
  按 onlyid key 解码后437B全零，reader只返回0x2F restore node，不解释这437B。
  但 full builder只是变换已有 backing，并不主动清零，raw-zero历史 producer/选择条件
  仍未知，所以禁止升 COMPLETE。
  current Windows SAFE6 分支传 non-null restore node 并执行 full rolling 的 blocker
  已修正：Windows/Linux producer 均确认 rolling 后把
  `LBA4+0x45/+0x46` 从 node 原样覆盖回来；Linux DWARF正式命名为
  `bDataToServer/bConnetServer`。22份原始盘证明 current/legacy 两类 profile 不能
  共用一个 flag 解码规则：current identity profile 使用 physical post-XOR flags，
  legacy profile 保留 generic ReadSector4 rolling 视图。inspect 已按该 profile-aware
  规则修正；Provision 使用官方 current SAFE6 full rolling + 两字节 post-XOR覆盖，
  validator 精确校验 full wire profile；历史 raw-zero short form 仅保留兼容读取。
  最新又用严格原始 Kingston `2026-08-27 17:30:24` 锁定一个关键反例：该盘已经是
  `OnllyID2Nd==main && HSerialCRC[5]==0` 的 current identity，却仍使用 physical
  raw-zero gap；同盘17:28:57快照逐字节一致。新增512B LBA4夹具 SHA-256
  `85141be31933e89976970ac18f44e1da8b77d3f57fdae1d857f8ea9d19a007ec` 和回归门禁，
  明确禁止再把 raw-zero/full-rolling 选择条件等同于 current/legacy identity 代际。
  两flag仍因缺最终业务consumer保持PARTIAL，raw-zero历史 producer/选择条件也仍未定位，
  严格完成字节数不增加。
- **LBA9 = 276 COMPLETE / 236 PARTIAL / 0 UNKNOWN = 53.9%**。
  EETU `reverse[104]` 已进一步闭合：Linux DWARF正式给出
  `tagEdpEDiskTmpUse.reverse[104]@+0x18`。Windows `SetTempUse` 清零目标结构后，
  从上层请求 `+0x44` 固定复制前102B；继续回溯 `BusManageImp::WriteNormalULabel`
  机器码确认其 SetTempUse 请求只写 begin/end/useCount，`sub_1009BAD0` 清的是另一块
  `ebp-0x9EC` 对象，不覆盖位于 `ebp-0xBD4` 的请求，因此前102B是明确的
  **writer-uninitialized opaque backing**。当前 runtime `ReadTempUseInfo` 把完整0x80
  缓存到 `CEdpDiskControl+0x1076`，`GetTempUseInfo/CheckTmpUse` 只读取时间和 useCount，
  登录递减次数后 `WriteTempUseInfo` 又整0x80透明写回；reverse 无独立业务读点。
  原始非零 LBA9 的 EETU 当前均观测 reverse=0，但零不是协议固定要求。
  因此前102B升COMPLETE；末2B此前已由 SetTempUse 显式零初始化闭合。
  EPPE `+0x188..+0x1FF` 120B 又完成 strict closure：Windows
  `SetPassInfoEx` 明确把 magic 后124B清零并只回填 `+0x04=minPassLen`；
  正式 `CUsbRegsiter::GetPassInfoEx` 解密校验后只返回该DWORD，
  `modfilesyscheck::ReadMinPassLenInfo` 也只消费 magic/minPassLen。
  两套 `EdpDiskCtrl` 虽保留 full-block compatibility helper，但 current DLL
  只导出 Create/Release factory，factory 返回对象的 vtable `0x1008021c`
  不包含 `GetPassExInfo/sub_10022AF0`；6/6原始 EPPE 的120B tail全零。
  因此该120B由PARTIAL升COMPLETE，语义为 **writer-owned zero tail**；
  未来若遇历史非零 tail 仍必须保留/报告，不能据此机械清零。
  SAPF trailing/backing 等继续无UNKNOWN但仍属PARTIAL。
  本轮进一步纠正中间区：Windows/Linux/vrvaud 三套 BuildSector6 都会把
  长 Dept `[60..NUL]` 写到 LBA9+0x80、长 User `[28..NUL]` 写到
  LBA9+0x100，所以这里不是纯 preserve 区。严格22盘有8份 long-Dept：
  4份 current join=60、4份 legacy join=59；官方 reader 两种接缝都支持，
  两组均重建为同一76B合法GBK Dept。join59 的旧 producer仍未找到，
  因此 +0x80..0xFF 暂不升COMPLETE；+0x100..0x17F 又与 SAPF/long-User
  profile重叠且22盘没有长User正向样本，继续PARTIAL。
- **LBA10 = 512 COMPLETE / 0 PARTIAL / 0 UNKNOWN = 100.0%**。
  EESI 前0x80 round-trip payload 中 magic 与两个16B卷标已闭合；
  本轮又把 `+0x04..+0x07` 4B 闭合为 **UsbSuspensionWnd lifecycle/control flag**：
  两套独立 `EdpEDisk.exe` 的 `OnInitDialog` 都先清零0x80B缓冲并显式写1后
  vtable+0x20 Get；两套卷标设置 `IDOK` handler 都清零完整0x80B、只填两个卷标，
  再 vtable+0x24 Set，因此保存路径明确写0。consumer 侧两套程序都加载
  `UsbSuspensionWnd.dll` 的 `Show/Destroy/SetParentWnd`，读回值0时走 Destroy，
  自动登录成功且值非0时进入刷新/Show链；唯一启用 EESI 的原始 SanDisk 实盘=1。
  这4B已满足边界、producer、值相关 consumer、原始实盘四项严格条件。
  后0x180已按 cross-generation unowned preserve/ignore 语义闭合为 COMPLETE：
  两代独立 EESI setter 都只替换前0x80并原样保留 tail，getter完全不暴露 tail，
  两个更老 build 连 EESI 路径都不存在。额外58份经 LBA6 CRC guard +
  LBA12 EDPF 双重验证的历史前部快照也 58/58 tail 为零。COMPLETE 不表示
  tail 必须为零；未来非零旧 profile 仍必须原样 preserve。
  本轮又把 `+0x28..+0x7F` 88B 闭合为 **EESI caller-owned compatibility
  extension**：两个独立 Ctrl build 对完整0x80B只做 structural round-trip，
  两套 official UI caller 都先清零完整0x80B、只填 `+0x08/+0x18`，
  current UserLogin/OnInitDialog/UsbSuspensionWnd 行为链均不消费这88B；
  SanDisk 与独立 Netac 两个正向 EESI profile 的88B也都为零。未来其它
  caller 若使用非零扩展值必须 round-trip，不能因为 current profile 为零而清零。

- **LBA6 = 423 COMPLETE / 89 PARTIAL / 0 UNKNOWN = 82.6%**。
  原352B UNKNOWN 已全部拆清。Windows sub_10013FD0 与 Linux
  BuildSector6@0x1CAAC 都先复制官方 UsbMainBSec，再覆盖明确字段；
  +0x40..4F、+0xC0..FF、+0x108..187、+0x1F4..1FB 共216B没有后续 overlay。
  Windows sub_100152A0 / Linux ReadSector6 在解析字段前对前508B整体做
  SAFE6 checksum 准入，严格22份原始盘又22/22逐字节等于官方模板，
  因而这216B升 COMPLETE。另136B旧 UNKNOWN 已恢复为
  Owner/User 32B 主槽、Office[64]、Label 56B 主槽。进一步下钻
  `UsbWriteParam(UsbLabelParam&)@0x1C362` 和项目自带
  `strcpy_s@0x1B9B0` 后确认：copy-constructor 不先清对象，strcpy_s 只复制到
  首个NUL并不清剩余 capacity，而 BuildSector6 随后把固定槽整宽 memcpy 上盘。
  因此 `m_autoid[16]@+0x70`、`m_UsbOffice[64]@+0x80` 和
  `m_usbLabel` 物理56B槽 `+0x188..+0x1BF` 的 post-NUL 非零字节都属于明确的
  writer-uninitialized backing，而不是未知隐藏字段。
  ReadSector6 只按 C-string 消费，前508B checksum 又覆盖全部物理字节；
  committed originals 中同一空 autoid 至少2种尾、同一空 Office 至少3种尾，
  同一 `江苏电力!SAFE6` Label 又至少有3种不同且非零的41B post-NUL backing。
  因此 autoid+Office 80B 与 Label 56B 均已升级 COMPLETE。Owner 仍受
  long-User 未见正向原始实盘影响，继续PARTIAL。
  Dept 64B 又进一步按真实 join profile 拆开：Linux current writer 的 short 分支固定
  memcpy 64B，long 分支写 marker `0x40245E2A + Dept前60B`；strict-original
  current Kingston join60 与 legacy Lexar join59 的解密 LBA6 Dept **前63B逐字节完全
  相同**，唯一差异是 `+0x3F`（current=Dept[59]=`A8`，legacy=`00`）。
  新回归同时锁定 short Dept 的 LBA6/LBA8 一致性和前63B内 nonzero post-NUL backing。
  因此 `+0x000..+0x03E` 63B 升 COMPLETE；只有 `+0x03F` 因 legacy join59
  producer/选择条件未知继续PARTIAL。
  `m_encrypt@+0x1F0..+0x1F3` 4B 也已闭合为 **write-only `!SAFE`
  label-generation metadata**：DWARF 正式字段位于 `UsbWriteParam+0x258`，
  Windows `RegsiterUsb` 的 `!SAFE` 5B 匹配明确产生1/0；Windows/Linux
  BuildSector6 均扩成DWORD落盘。读取侧正式 `UsbLabelParam` 没有该成员，
  Windows CheckLabel/Linux ReadSector6 与已扫 runtime 不按值消费，仅整扇
  checksum覆盖其物理字节；strict originals 22/22和独立SanDisk均为1。
  Dept/Owner 长值的 continuation 还确认落在 LBA9+0x80/+0x100，
  并已用 join60/join59 双profile实盘门禁锁定。当前全 LBA0–12 已无 UNKNOWN，
  目前全局仍有2821B PARTIAL，绝不能把“UNKNOWN=0”当成全部协议完成。

- **LBA7 = 512 COMPLETE / 0 PARTIAL / 0 UNKNOWN = 100.0%**。
  真实物理 LBA7 已锁定为 **3×0x40 packed EDPF + 14B pass-info@+0xC0**，
  不能用 `libcemsfilesyscheck.so` 的 0x48/72B natural ABI 直接解释盘面。
  v0x0064 legacy wrapped key `entry+0x38..0x3F` 已完整闭合：
  `fold32("0000aaaa")=0x91919191`，两个 DWORD 分别 XOR 解包；
  22份 original real-device 中全部28条非零 type2/type4 legacy entry **28/28**
  通过 `CRC32_bare(file_key8)==FileKeyCRC`。producer/consumer/写回链为
  `ChangePwd -> sub_10026050 -> sub_10028DB0 -> sub_100125B0
  -> SavePartionSector/sub_10028580 -> sub_10010FC0 -> WriteFile(LBA7)`。
  本轮继续闭合 `+0x0CE..+0x1FF` 306B：Windows `sub_10010FC0` 明确先
  memset staging，仅写0xC0 table+0x0E tail后整扇rolling；`sub_10010B40`
  解密整扇但只返回前0xCE；严格22份原始盘 22/22 解密后这306B全零。
  因此306B由 UNKNOWN 直接升 COMPLETE。复核总账时还发现此前已闭合的
  entry0 `NeedDisturb` 4B（producer + `NewCheckDisTurbUsb(*)` consumer + 22/22实盘）
  被正文标为 COMPLETE 却漏算进总数，现已纠正。LBA7 已无UNKNOWN。
  本轮又把 entry0 的行为链继续闭合到实际磁盘动作：`SetProtect` 在
  `NewCheckDisTurbUsb(*)` 判定后调用 `sub_1006ff80 -> sub_1006e580`，
  精确覆盖 LBA0 `+0x1BE..+0x1FD` 的64B MBR partition table；静态模板仅含
  `type=0x04,start_lba=66,sector_count=1` 的占位 entry。`UnsetProtect` 反向走
  `sub_1006ffd0 -> sub_1006e9b0`，从 LBA2 读整扇恢复到 LBA0 并刷新磁盘属性。
  因此 entry0 `NeedDisturb` 可明确描述为 **MBR scramble/descramble gate**。
  entry-local Version 12B 与 entry1/entry2 NeedDisturb 8B 又继续按 compatibility
  metadata 生命周期闭合：current writer 分别给出 Version=0 与 positional
  NeedDisturb `(1,1,0)/(1,1)` profile；Windows old/new converter 双向保留；
  两版 `vrvaud_c` 只有 entry0 NeedDisturb 行为读取；Linux `CDiskReader` 的
  GetTagPartitionInfo/DecryptFileKey/CheckFileKeyCrc/ReadFileSysSector0/
  DecryptFileSysSector0 均不读取 Version 或 NeedDisturb。committed originals +
  独立 SanDisk 全量门禁已锁定这些 profile，因此20B升 COMPLETE。
  最新又从独立 Linux `checkdiskback` 找到 pass-info
  `bNoUsbChkPasSafe(+0x0A)` 的真实 consumer：
  `Update_EDPEDISKSHOWPARAM@0x406B70` 执行
  `pass+0x0A == 1 ? showparam+3=0 : showparam+3=1`；
  `CreateSafe6TmpPolicyFile` 随后将其写入 CRC/A6B0 加密策略文件，
  `EdpEDiskBack` 与 `linuxedpedisk` 两套 `Safe6PolicyFile` 独立恢复。
  22份原始盘为18×0+4×1，且22/22 LBA7/LBA12一致，因此 LBA7/LBA12
  各1B升级COMPLETE。最后两个
  `ShareBackuppromptPeriod / EncryptBackuppromptPeriod` BYTE 也已继续闭合：
  Linux DWARF 明确它们是 `edpdiskglobal.h:164/165` 的正式 BYTE 字段；
  current writer 对完整14B pass-info zero-init 且不覆盖 `+0x0C/+0x0D`；
  四个不同哈希的 Windows EdpDiskCtrl reader 与 Linux checker 都只结构搬运/
  保存完整14B，没有这2B的值相关业务读取；两代 `vrvaud_c` 的同名 backup policy
  已确认是独立 string/DWORD 链。committed originals 的 LBA7/LBA12 两副本逐盘0/0一致，
  全目录19个去重真实 LBA7 profile（含 v0x0064/v0x0206）仍19/19=0/0。
  因此2B按 **dormant compatibility bytes** 升 COMPLETE，LBA7 整扇闭合。
- **LBA8 = 476 COMPLETE / 36 PARTIAL / 0 UNKNOWN = 93.0%**。
  `+0x080..0x1FF` 384B 已按 **LBA8 dynamic ELABEL + encrypted backing + preserved tail**
  整体闭合：Windows `sub_100148d0` / Linux `BuildSector8` 都只写17-key ELABEL+NUL，
  对预读旧 LBA8 的动态前缀原地加密；NUL后到 encrypted_len 是既有 encrypted backing，
  后部是 physical preserve tail。Windows/Linux semantic reader 都只消费
  注册侧 Windows/Linux reader 回填 `Label/GLab/Dept/User/Autonum/Rmark/Unit` 七键；
  继续追到运行时 `EdpEDiskCtrl::sub_10016260` 后确认它会解析全部17个ELABEL键，因此另十个
  是当前实盘为空的 compatibility wire slots，并非全产品 ignore。该 runtime reader 还复制
  `HDSerialInfo/UsbOnlyInfo` 到 `CEdpDiskControl+0x2D0` 标签结构，但未找到两字段值相关后续读点。
  继续取得并MD5核验两份2020官方旧件：`EdpEDiskCtrl.dll` v3.6.10.18
  (`95a06e0d466ba40a7d5c0e6a409e2114`) 与 `CEMSUsbRegsiter.dll` v19.11.4.1
  (`783d01f19e998a514834bc5e5f4249ad`)。后者的可达 `sub_10007DF0` 明确先调
  `UsbTools` ordinal4，0时fallback ordinal3，把结果写 `HDSerialInfo@+0x14`，并作为
  `%08x%08x` 第二DWORD写入 `UsbOnlyInfo@+0x1E`。本机 `UsbTools.dll` 导出表现已精确
  锁定 ordinal4=`EDP_DiskNumber`、ordinal3=`EDP_DeviceNumber`；二者分别 thunk 到
  `DeviceNumber.dll` ordinal3/ordinal1，禁止再把两层DLL的ordinal编号混为一谈。主路径为：
  枚举PhysicalDrive0..3，ATA IDENTIFY取20B serial，word-swap+trim，跳过失败/全零，
  无分隔拼接后做标准CRC32(poly 0xEDB88320, initial=0)。因此已定位一个真实非零
  HDSerialInfo producer family。
  fallback `EDP_DeviceNumber` 也已闭合：先无分隔追加同一批规范化 ATA serial，再追加
  network helper mode=1 的 MAC 串；该模式跳过全零MAC及同时含 `VIRTUAL`+`VMWARE`
  的 VMware virtual adapter，每条序列化为
  `MACAddress<i>=<12位大写无分隔MAC>\r\n`，最后追加 `MACCount=N\r\n`，再用同一
  reflected CRC32 求值。没有有效MAC时 network blob 为空。
  2020 runtime又提供negative-consumer硬证据：整DLL不导入DeviceNumber；标签读入后
  后续 direct-member 只消费结构前14B，唯一 `this+0x173C` 反而作为0x104B路径缓冲区被覆盖。
  但该2020 writer会同步生成非零 UsbOnlyInfo，与16份 strict legacy 原盘
  “HDSerialInfo非零 + UsbOnlyInfo全零”不一致，所以 exact legacy writer仍未闭合，36B计数不变。
  严格22盘17-key顺序一致且十个 compatibility key 全空。当前仅 `HDSerialInfo` 4B 与
  legacy `UsbOnlyInfo[32]` 32B 继续PARTIAL。
  旧账本把22盘当前最大正文之后的102B机械记成 UNKNOWN，这是错误的固定边界模型。
  Windows `sub_100148d0` 与 Linux `BuildSector8@0x1D602` 都只写/加密动态前缀，
  不清零后续输出 backing；严格22盘重新复算得到
  `logical_end=0x148..0x183`、encrypted prefix=`0x150..0x190`。
  因而 `+0x80..+0x1FF` 的正确模型是“ELABEL / 最后一块加密padding /
  preserve-existing tail”动态三态，边界由 LLGB length 决定。inspect 已有
  synthetic nonzero-tail 回归，确保前缀后的物理字节原样保留。102B由UNKNOWN降为
  PARTIAL，不增加COMPLETE。另已补齐一个此前实盘未覆盖的边界：logical_len 恰好
  16B对齐时，官方 writer 仍按 `(logical_len/16+1)*16` 多加密一块以容纳结尾
  NUL；inspect 原先普通 round-up 会少解16B，现已修正并有专门红→绿回归。
  current `UsbOnlyInfo` producer 也已追到真实 Windows 调用栈：
  `RegsiterUsb@0x1003BD3E..0x1003BD88` 先压入 `object+0x698` main onlyid，
  再按值复制完整 `0x2AC` UsbLabelParam 后调用 `sub_100148d0`；
  后者以 `[ebp+0x2B8]` 执行 `sprintf("%08x%08x", main_onlyid, 0)` 并写
  `LBA8+0x1E UsbOnlyInfo`。严格22份按 LBA4 identity profile 重算：
  6/6 current 均为该格式且 HDSerialInfo/MacInfo=0；16/16 legacy 均
  UsbOnlyInfo为空并保留历史非零 HDSerialInfo；committed-original 回归现在也显式断言
  legacy `HDSerialInfo != 0`。进一步把42B混合区拆开后，
  `MacInfo[6]@+0x18..+0x1D` 已满足独立闭合条件：官方字段名明确、
  Windows/Linux header 都显式零初始化、semantic reader 完全跳过，
  且 current/legacy 22/22 均为6B零，无已知 profile 分叉，因此6B升COMPLETE。
  `HDSerialInfo@+0x14..17` 与 `UsbOnlyInfo@+0x1E..3D` 仍因 legacy producer/
  历史 consumer 缺口保持PARTIAL。
- **LBA12 = 464 COMPLETE / 48 PARTIAL / 0 UNKNOWN = 90.6%**。
  主运行时盘面固定为 3×96B packed entry；`Reserved[7]@+0x59..+0x5F`
  已由官方字段名、writer 零来源、negative consumer 和 66/66 原始 entry 闭合。
  相邻 `+0x48..+0x57` 也已独立闭合，但**不是 Reserved**：104B natural ABI
  正式命名为 `EncryptFileKey32[16]`。旧72B ABI根本没有该槽，old→new converter
  不填它；current Windows packed writer 对3×96B整表先清零且不覆盖这16B。
  checker 的 DecryptFileKey/CRC/filesystem decrypt 均不读 natural +0x50；
  packed `libedpedisk.so::PartitionHeader` 虽按值结构缓存完整96B，但精确符号边界审计
  证明映射到 object+0x88/+0x90 的两个QWORD只有 ctor 写入，没有算法读点；
  正对照主 wrapped16 映射到 object+0x78/+0x80 并被 SMS4/AES128/OldEdp decrypt
  实际消费。严格22盘全部66条 entry 的该槽66/66为零。因此三个 entry 共48B
  升 COMPLETE，按 **EncryptFileKey32 compatibility slot structural-cache /
  negative-semantic-consumer closure** 建模；未来独立ABI若出现非零值应结构保留，
  不能强制清零。
  v0x0206 默认密码 mode2 wrapping 也已独立闭合：
  `"0000aaaa" -> sub_10040400 -> "LtSWi[2f)j"`，
  MD5 后走标准 SM4；43/43 默认 mode2 原始 entry 可独立解包并通过 FileKeyCRC。
  另外3×`Version@+0x04` 共12B与 entry1/entry2 `NeedDisturb@+0x10` 共8B
  已按 compatibility metadata 生命周期闭合：current 3×96B producer 对 Version
  保持 zero-init；NeedDisturb positional profile 为 `(1,1,0)`；Windows/Linux packed
  runtime 对这些字段只结构携带，行为 consumer 只命中 entry0 NeedDisturb；committed
  originals 与全树22份完整历史备份复算均为 Version `(0,0,0)` / NeedDisturb `(1,1,0)`。
  因此新增20B COMPLETE。当前唯一剩余是3条 `wrapped16@+0x38..+0x47` 共48B：
  mode1/mode3 等已知 profile 仍缺正向原盘，不得把整个 wrapped16 升 COMPLETE。
  继续反向追 writer 后已确认 mode1/mode3 不是死代码：`WriteNormalULabel`
  读取请求 `arg+0x7E8`，按 1→mode1、2→mode3、其它→mode2 写入
  `CUsbRegsiter this+0x6EC`；`CreatePartitions` 再按该值选择 A7F0/SM4/AES
  并写 `entry+0x58 EncryptMode`。上层 `LabelInfo+0x7E8` 的日志名为
  `crypt`；制标 UI 也有 `SMS4/AES/AES_CROSS` 三项并把 currentIndex 写入
  `normalDetail.algorithm`。但 `normalDetail.algorithm -> LabelInfo.crypt`
  的直接桥接点还没找到，不能把两端数据流误写成已经完全接通。
- **LBA11 = 512 COMPLETE / 0 PARTIAL / 0 UNKNOWN = 100%**。
  正常注册 writer/reader 使用 `DISK_GEOMETRY_EX.DiskSize`；
  `UDiskLabelRepair.dll::CLabelRepair::Repair` 的 LBA11 检查/重写路径使用
  `IOCTL_DISK_GET_DRIVE_GEOMETRY` 得到传统 `DISK_GEOMETRY`，按
  `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector` 计算 CHS capacity。
  `sub_10008820 -> sub_10003BD0` 用该容量校验 LBA11，失败后
  `sub_10008950(ReWrite11Sector) -> sub_10003A40` 用同一容量重建 LBA11；同一
  Aigo U335 rev_pmap 的 CHS/exact 双真实捕获与两条官方路径吻合。因此后半252B已
  从 PARTIAL 升 COMPLETE，不再重复追 CHS profile。
- **LBA0 = 112 COMPLETE / 400 PARTIAL**。
  legacy MBR `+0x1B5..+0x1B7 = 2C 44 63` 已闭合为三个错误消息指针低字节；
  current zero bootstrap、legacy `UsbMainBSec` 与 Aigo L8302 第三 profile 已进一步
  分型。Aigo 前400B已精确匹配 CEMS 随附 `Netac_USB_API.dll/hardware.dll` 等8份
  二进制模板，且 `Netac_USB_API.dll::sub_10003880` 用 `rep movsd(0x80 dword)`
  整扇生成该 MBR；legacy `UsbMainBSec` 也找到
  `CUsbRegsiter::UnRegsiterUsb -> LBA0` 的直接写回链。当前仍缺旧注册版本为何在
  已注册实盘保留 legacy bootstrap，以及何种历史入口真正选择 Netac Format 的 profile
  selection，因此前400B bootstrap 主体仍保持 PARTIAL。最新静态追踪已确认
  `BusManageImp::WriteLabelImp -> SafeUsbRegsiterCems.dll!GetUsbTegsiterObj
  -> CCEMSSafeUsbRegsiter::UsbFormat`，但这里的 `UsbFormat` 实际只做
  sectorManage/设备信息 `0x52 -> 0xA2` 判定及
  `BackPassWord(Office)` 的 IIR/password 预处理；紧随其后的
  `RegsiterSafeUsb` 也未调用 Netac Format。独立地，
  `usb20dll.dll!_IF_DiskFormat -> NewUsb20.dll!FormatExA_NetacAPI`
  已精确闭合，但当前官方包没有找到主制标链对 `IF_DiskFormat` 的普通 import
  或 `GetProcAddress` 名称引用。后续必须找真正的历史升级/量产调用点，不能把
  两条链因同名“Format”直接拼接。
  本轮又把原先机械并入大区的两段尾部拆开：`+0x190..+0x19F` 16B 与
  `+0x1A4..+0x1B4` 17B 在 current SAFE6 中均为 unowned preserve，Linux
  `BuildSector0` 也不写；legacy `UsbMainBSec` 与 Aigo/Netac 模板均生成零，
  16-bit bootstrap / current 注册准入 / Repair 都不读取。严格22盘与扩展57份
  完整历史快照两段均全零，因此共33B按 cross-profile unowned preserve / historical-zero
  compatibility region 升 COMPLETE。`+0x1A0..+0x1A3 SectorSize` 也已继续闭合为
  **optional SAFE1 / legacy compatibility overlay**：Windows `sub_10013F10@0x10013FB6`
  只在 SAFE1 分支写 `m_nSectorSize`，legacy `UsbMainBSec` 模板为512；Aigo/Netac
  整扇 template 该槽为0，current SAFE6只preserve，Linux BuildSector0只使用 sector
  size计算分区而不落盘。current CEMS 对该偏移唯一真实 sector-data 访问就是 writer；
  repair/Linux/runtime 均无语义 reader，EdpDiskCtrl 的 `+0x1A0` 命中已确认是
  vtable/object 偏移。扩展历史中47份相同 legacy bootstrap 分为10×0、37×512；归一化
  SectorSize/disk-signature/partition-table 后47/47整个LBA0 SHA-256均为
  `2c8877b90c5d42d73f17c511ef5984efc8135543bda0746ef54f347320d78e8f`，证明它只是
  独立 overlay。committed fixtures 同时锁定0/512双态，因此4B 已升 COMPLETE。
  `+0x1B8..+0x1BB` 已继续闭合为 standard Windows MBR disk signature：
  producer 是 `CREATE_DISK_MBR.Signature -> IOCTL_DISK_CREATE_DISK`；Windows
  `DRIVE_LAYOUT_INFORMATION_MBR.Signature` 正式把它定义为唯一标识 MBR disk 的
  drive signature。当前 `CEMSUsbRegsiter.dll::fcn.10046320` 唯一的
  `IOCTL_DISK_GET_DRIVE_LAYOUT_EX(0x70050)` 路径只读取
  `PartitionStyle@+0x00` 与 `PartitionCount@+0x04`，不读取 `Mbr.Signature@+0x08`，
  因此 EDP 对该标准字段没有额外业务语义；22盘19种真实值与新增 fixture 门禁补齐
  实盘证据，4B 已由 PARTIAL 升 COMPLETE。
  相邻 `+0x1BC..+0x1BD` 2B 也已从旧的“标准 reserved word + 全零样本”继续追到
  **unowned compatibility word**：current SAFE6/Linux BuildSector0 都不写，legacy
  `UsbMainBSec` 与 Aigo/Netac 模板均为 `00 00`；16-bit bootstrap、current EDP
  准入/repair/drive-layout 路径均不把它作为业务字段读取。严格22盘与扩展57份完整
  历史快照全部为 `00 00`，因此这2B也满足 producer/preserve + negative consumer +
  实盘四证，升级 COMPLETE；未来未知非零值仍应 preserve，不能据此强制清零。
- **LBA3 = 整扇 PARTIAL**。
  EDP 注册链只 preserve existing，当前 EDP reader 不解析；strict 22盘仍是21零 +
  1份 Kingston `this is mp mark\0` 制造 payload。扩展只读扫描两个备份目录60份
  `.bin` 后，又发现第二种真实历史 Kingston marker profile：两类均有
  `+0x001=01` 与同一尾 marker，但 `+0x020..027` 分别为
  `a8 82 a4 22 00 20 02 16` / `b5 7e 9c 45 00 80 00 14`；同型号还存在全零
  profile。已加入独立512B证据夹具和差异回归，明确中间8B不是固定模板。
  新增外部证据已把尾 marker 收敛到 **Phison FW.BIN / MPALL** 制造生态：
  USBDev.ru 的 Phison firmware 资料与 Falcon Sandbox 的
  `MPALL_F1_9000_v372_0B.exe` 均出现同一 `this is mp mark`，后者同时出现
  `C:\\PhisonLog` 与 Phison controller 型号串。strict 非零盘是0951:1666、
  62008590336B，但公开同 identity/capacity 同时存在 PS2307 与 PS2309，
  因此不能锁死具体 controller。进一步交叉两代 MPALL 与一代 UPTool 静态结果，
  已把 producer 函数族定位到 `CBaseController::WriteF2Mark`（3.72/5.03 均存在），
  3.72 还出现 `CU32SSBaseContoller::WriteF2Mark` 与 `F1-F2 MARK`。但函数内部
  sector offset/staging layout、`+0x001/+0x020..027` 字段 store 与 firmware
  consumer 仍未取得，因此完成字节数不增加。
- **LBA4 current writer layout 已闭合到机器码**：
  `OnllyID2Nd=main onlyid`，`HSerialCRC[5]` 来自对象五个 DWORD；
  当前 `WriteNormalULabel` 不填 `HDOnlySerial[5]`，所以 current profile 为0；
  14/22 固定 `1D29,7B,4DD,79,7C` 与 2/22 高熵旧 profile 的 producer 仍未知。
- **LBA6 GSerial/BeiZhu 已纠偏为 C-string + opaque post-NUL backing bytes**，
  不能按固定16B零 padding；`+0x1E0..0x1EF` 20/22零、2份 legacy 非零，仍待追。
- **LBA8 static header**：ToolVersion、Labversion、writeTime、Reserved[64] 已闭合；
  writeTime 是单调时钟毫秒，不是墙钟时间。
- LBA5：整扇512B不是字段结构，而是 opaque write-protection probe scratch。
  官方注册 writer preserve existing bytes；两版 EdpDiskCtrl 都只执行
  “读整扇→同字节写回→检查 ERROR_WRITE_PROTECT(0x13)”；22/22原始盘当前全零，
  但零不是协议固定要求。
- LBA10：EESI `+0x08..0x17`、`+0x18..0x27` 两个16B槽已闭合为
  type2/Share“交换区”卷标和 type4/Encrypt“保密区”卷标。
  `UserLogin` 分别把两者传给 `SetVolumeLabelA`。
  **LBA10 当前为 512 COMPLETE / 0 PARTIAL / 0 UNKNOWN = 100.0%**：
  `+0x04..07` 已由两套独立 `EdpEDisk.exe` 的启动写1 / 标签设置保存写0 producer，
  `UsbSuspensionWnd.dll` 的 Destroy 与刷新/Show 值相关 consumer，以及原始 SanDisk
  实盘=1闭合为 COMPLETE；
  后384B已按 cross-generation unowned preserve/ignore 语义闭合。两代独立
  EESI setter 都只替换前0x80并原样写回 tail，getter完全不暴露 tail；
  两个更老 build 连 EESI 路径都不存在。committed originals + 独立 SanDisk
  继续锁定真实样本，额外58份经 LBA6 CRC + LBA12 EDPF 双重验证的历史快照
  也为58/58 tail零。COMPLETE 不代表 tail 必须为零，非零旧盘仍必须原样 preserve。
  EESI `+0x28..7F` 88B 已按 **caller-owned compatibility extension** 闭合：
  两套卷标设置 `IDOK` handler 都先清零完整0x80B再只填两个卷标；两版 Ctrl
  对完整0x80只做 Get/Set structural round-trip，current UserLogin/OnInitDialog/
  UsbSuspensionWnd 行为链均不解释这88B。因此 COMPLETE 表示“caller 可扩展、底层
  必须保留”的生命周期闭合，不代表未来值必须为零，也不臆造内部字段名。
  新增补充实盘：2026-08-04 Netac OnlyDisk 历史6656B捕获在独立
  `device_id=disk&ven_netac&prod_onlydisk&rev_0000` 下通过 LBA6 CRC、LBA7/LBA12
  EDPF 自洽校验，LBA10 同样解出 `EESI/+0x04=1/交换区/保密区` 且后88B全零；
  已加测试门禁；该历史样本仍不并入22份严格生成参考，但作为第二个正向 EESI profile
  支撑 compatibility-extension 生命周期，不改变严格主样本计数。
- LBA1/LBA2：旧文档“保留/全零”结论已纠偏。Linux 官方
  `BuildSector1_Gpt/BuildSector2_Gpt` 和 Windows GPT parser 均证明二者存在
  GPT Header / GPT Partition Table profile；但22/22当前原始 SAFE6 参考都全零，
  没有正向GPT实盘，因此两扇区各 **512B 只能标 PARTIAL，禁止升级 COMPLETE**。

目标：让一块普通全新 USB 能生成并安全写入 EDP/cems 前部 metadata。协议、备份、inspect、Provision 统一只处理 **LBA0–12（13 sectors / 6656B）**。第一阶段不格式化数据区，不写 LBA12 之后区域。

先完整阅读 `docs/PROVISION_NEW_USB_PLAN_2026-09-19.md`、`docs/RELEASE.md`，再检查 git 状态。允许连接 Mac，Phase 0 优先用现有备份和真实盘做**只读**协议审计。不要把现有 `apply` 直接改造成 provision；必须先建立纯 `ProvisionSpec/Profile/Image/Validator`。

严格 Phase 0→7，测试先行、小 commit、及时 push。最新 Phase 0 结论包括：onlyid 官方链为 `CoCreateGuid -> CRC32_bare(raw16)`；LBA11 为 `DRKB + random252`；LBA12 是整扇连续 A6B0/A7F0；官方主注册写集固定为 LBA0–12。其余未知 reserved/dynamic bytes 不得猜测或随意清零。CLI/TUI 必须复用 application/service 和现有写盘安全链。

## 当前逆向进度（交接重点）

当前最容易误踩的是 **LBA7/LBA12 ABI 与 key profile**：

- LBA7 physical old table = **0x40 packed stride**；Linux
  `libcemsfilesyscheck.so::tagEdpPartionInfo` = **0x48 natural ABI**，中间多4B
  alignment hole。两者字段名可参考，但 offset 不能混用。
- LBA7 v0x0064 old table 的 `FileKeyCRC@+0x34` 已早先 COMPLETE；
  本轮新闭合的是 `wrapped8@+0x38..+0x3F`，只新增 **24B COMPLETE**，
  不得再次把 CRC 4B/entry 重复计数。
- LBA12 physical current table = **0x60 packed stride**；Linux 检查组件另有
  0x68 natural ABI。主盘面表尾固定在 `+0x120`，不是 `+0x138`。
- LBA12 `+0x38..+0x47 wrapped16`：当前22盘真实使用的
  `v0x0206 + mode2` 已闭合；`oldSM4` 已证明只是同一标准SM4的实现选择，
  不是独立 wire profile。扩展历史只读复核共52份有效捕获、33个不同
  SHA-256，去重后仍只有25份 mode tuple=`0,2,2` 与8份=`2,2,0`。
  底层 `crypt` 到 mode1/2/3 的 writer 可达性已经闭合，但
  `normalDetail.algorithm -> LabelInfo.crypt` 的桥接仍缺直接写点；
  且 mode1/mode3 依然没有正向原盘，因此整个16B字段继续保持 PARTIAL。
- LBA12 `+0x48..+0x57` 已闭合为 `EncryptFileKey32[16]` cross-generation
  compatibility slot；相邻 `+0x59..+0x5F` 是独立 COMPLETE 的 `Reserved[7]`。
  两者都已完成，但语义完全不同，禁止重新混成一片 zero padding。
- pass-info 14B 已全部闭合：
  `+0x0A bNoUsbChkPasSafe` 已由独立 `checkdiskback`
  `Update_EDPEDISKSHOWPARAM` 的值相关策略映射和两套 Safe6PolicyFile 消费链闭合；
  `+0x0C ShareBackuppromptPeriod`、`+0x0D EncryptBackuppromptPeriod`
  已由 current-zero producer、四代 Windows/Linux structural-preserve、
  独立 backup-policy 排除与跨 v0x0064/v0x0206 实盘0/0证据闭合为 dormant
  compatibility BYTE。LBA7 因此已 512/512 COMPLETE。

下一位 AI 优先顺序：

1. **先检查 git 状态并读主账本，不要重复已完成分析。**
   当前 HEAD 必须包含 `50417a9` 或更新提交，branch 与 origin 同步且 worktree clean。
   必须先读：
   `AGENTS.md`、
   `docs/HANDOFF_PROVISION_NEW_USB_2026-09-19.md`、
   `docs/PROTOCOL_BYTE_TRACE_2026-09-19.md`、
   `docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md`。
   禁止 `reset/clean`。
2. **LBA4 `+0x45/+0x46` 与 current canonical 已完成本轮修正，不要回退。**
   已确认：
   - Linux `BuildSector4@0x1D08E` 与 Windows `sub_10014550` 都在 full rolling 后
     post-XOR 写回 `node+0x2D/+0x2E`；
   - Linux DWARF字段名是 `bDataToServer/bConnetServer`；
   - Windows/Linux `ReadSector4` 都不补偿该例外，只 generic rolling + memcpy node；
   - 严格22盘 22/22 physical flags != generic rolling flags；
   - inspect 已显示 producer-side physical flags；Provision/validator 已改为 current
     SAFE6 full rolling + post-XOR flags；历史 raw-zero form仍兼容读取。
   后续若继续追 LBA4，应只追这两个字段的**最终业务 consumer**，找到之前仍PARTIAL。
3. **LBA7 已 512/512 COMPLETE**：
   - 最后 pass-info `+0x0C/+0x0D` 已按 dormant compatibility-field 生命周期闭合；
     若未来发现非零旧 profile，必须 preserve/report 并扩展 profile，不能机械清零。
   这2B的 current producer 已进一步锁到机器码：`CreatePartitions/sub_1003DB50`
   在 `0x1003DC16..0x1003DC26` 显式清零完整14B pass-info，后续 store 最远只到
   `+0x0A`，所以 `+0x0C/+0x0D` 是 current writer-owned zero。两代 `vrvaud_c`
   的 `BackupPromptInfo` 已确认是独立 policy 来源；其0xC0 old-table 全局不包含
   pass-info tail，禁止仅凭名称相似把它当这2B的 consumer。后续四代 reader +
   DWARF + 扩展历史 profile 证据已证明该2B在已覆盖实现中是 dormant compatibility
   bytes，因此已升级 COMPLETE；不再追一个当前不存在的 active period consumer。
   本轮已重新核实两版 `vrvaud_c` 全局 old-table 都是3×0x40 packed，且只有
   entry0 NeedDisturb 有行为 xref；Version、entry1/2 NeedDisturb 已结合 current
   producer、双向 converter structural-preserve、Linux CDiskReader negative-semantic
   consumer 与真实 profile 全量门禁按 compatibility metadata 模型闭合，不得再回退成 Reserved。
   不要再重复分析 wrapped8；其 24B 已 COMPLETE。
4. **LBA4 legacy HSerialCRC[5] producer**：
   当前 writer machine code 已闭合，但旧 14/22 固定
   `1D29,7B,4DD,79,7C` 与2份高熵 profile 的生成源仍未知。
   `edpuniqueid` 的 `Drive%dSerialNumber`、`DeviceNumber.dll::EDP_DiskNumber`
   已查过，当前没有建立到五槽 writer 的证据链；不要重复把它们硬接。
5. **LBA6 legacy +0x1E0..+0x1EF**：
   20/22零、2份旧格式非零；current template 为零，但旧 producer/consumer未知。
   GSerial/BeiZhu 的 post-NUL 残值已明确是 backing bytes，不要再按 padding。
6. **LBA8 ELABEL 与动态头剩余字段**：
   static ToolVersion/Labversion/writeTime/Reserved 已 COMPLETE；
   动态 ELABEL 384B 已闭合。`HDSerialInfo` 已找到2020非零 producer family及
   `EDP_DiskNumber=CRC32(规范化ATA serial无分隔拼接)` 主算法，2020 runtime也已证明不按值消费并会复用覆盖该槽；
   但该代同时写非零 UsbOnlyInfo，尚不能解释 strict legacy 的“HDSerialInfo非零 + UsbOnlyInfo全零”。
   `EDP_DeviceNumber` fallback 已补完，且 `UsbTools` ordinal4/3 与 `DeviceNumber` ordinal3/1
   两层 thunk 已纠偏。下一步只追更早 exact writer/profile selection；不要把2020过渡profile冒充16份legacy生成源。
7. **LBA10 只剩 `+0x28..+0x7F` 88B**：`+0x04` 已闭合为
   `UsbSuspensionWnd lifecycle/control flag`，不要回退到“版本1/时间戳”或PARTIAL。
   两个16B卷标槽也已 COMPLETE。当前88B已有两套官方 UI zero producer、底层
   Get/Set opaque round-trip，以及 SanDisk + 补充 Netac 两个 EESI 零 profile；
   继续目标是正式字段声明、非零历史 profile 或值相关 consumer，缺任一关键环节前
   不得把这88B机械升级为 reserved/COMPLETE。
8. **LBA0 bootstrap / LBA1-LBA2 GPT 正样本**：LBA0分区表+55AA及3B legacy
   message pointer 已闭合。进一步确认 current `RegsiterUsb` 在最终13扇区写入前
   无条件清零 `LBA0+0x000..0x18F`，`UDiskLabelRepair::ReCreate0Sector` current
   新建路径也清零同一区；而多份 legacy 原盘前400B逐字节等于官方
   `UsbMainBSec@0x100E7220`，SHA-256 =
   `4eeee8d52f8b58d9a1fa35b63a14c8c5dba1b2717eaa44e6fb1ff0327ccbe5ed`。
   另有 Aigo L8302 第三种特殊 bootstrap，其 Netac 512B template producer 已闭合。
   **不要因此升级 COMPLETE**：仍需找到 historical legacy registration profile 选择，
   以及真正调用 `usb20dll!IF_DiskFormat -> NewUsb20!FormatExA_NetacAPI` 的
   历史升级/量产入口；current `WriteLabelImp -> CCEMSSafeUsbRegsiter::UsbFormat`
   已证明不是这条直接调用链。LBA0 现在只剩前400B bootstrap profile-selection。若能找到真实原始 GPT EDP 盘，可用于把 LBA1/LBA2
   从 PARTIAL 继续细分；本轮已额外只读扫描 `u_disk` 下3896个大于13扇且
   小于1GiB的候选文件，没有任何文件在 LBA1 起点出现 `EFI PART`。在取得新
   外部实盘前不得再把本地 synthetic builder 输出冒充正向证据。
10. **LBA3 Phison MP/FW payload**：当前已明确 EDP 只 preserve/ignore，且
    `this is mp mark` 已收敛到 Phison FW.BIN / MPALL 制造生态，producer 函数族
    又进一步定位到跨版本 `CBaseController::WriteF2Mark`。后续只追该函数内部
    512B staging/sector write、`+0x001/+0x020..027` 字段定义和 controller firmware
    consumer；不要再重复证明 EDP 不使用它，也不要因 0951:1666/容量相同就把
    controller 锁死为 PS2307 或 PS2309。

每得到一批闭合结论，都要同时更新
`docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md` 和
`docs/PROTOCOL_BYTE_TRACE_2026-09-19.md`，必要时更新 inspect，并在
`tests/provision_protocol_audit.rs` /
`tests/protocol_documentation_contract.rs` 增加回退门禁；每批单独小 commit + push。

## 原始实盘与旧文档使用规则

- 旧 `/Users/zhangyuxi/Desktop/u_disk/docs`、`analyze/**` 文档**只能用作函数名/
  地址/关键词索引**，不得把其中结论直接抄入主账本；本轮已经多次发现旧结论不完整
  或错误，例如 LBA6 `+0x1CA`、LBA10“时间戳”、LBA1/LBA2“纯保留区”。
- 当前 generation reference set = **22份只读原始样本**：
  21份 `nopwd_tool/backup` 中按内容排除转换盘后的原始完整快照 +
  1份独立 SanDisk encrypted 原始样本。
- `tests/fixtures/protocol` 的7份只是 CI curated subset，不是完整逆向语料。
- 免密转换盘、edpcli 自生成盘只能做回归，不得作为“官方生成语义”的实盘依据。
- 结论升级顺序固定：
  **UNKNOWN → PARTIAL → COMPLETE**；
  COMPLETE 必须同时具备字段/区域边界、官方 producer、官方 consumer/行为、
  原始实盘验证。
- “全零”“固定值”“字段名已恢复”“能生成”“能解密”单独任何一项都不足以 COMPLETE。

## 交接时验证状态

本次交接实际验证：

- `cargo test --test inspect --locked`：**20/20 PASS**；
- `cargo test --test provision_generate --locked`：**3/3 PASS**；
- `cargo test --test provision_validate --locked`：**6/6 PASS**；
- `cargo test --test provision_protocol_audit --locked`：**48/48 PASS**；
- `cargo test --test protocol_documentation_contract --locked`：**3/3 PASS**；
- `cargo test --test golden --locked`：**14/14 PASS**；
- `git diff --check`：PASS。

本轮强化的 LBA4 门禁：

- `lba4_short_form_must_be_decoded_by_regions_not_by_zero_bytes` 必须同时覆盖
  raw-zero 与 rolling-encrypted-zero 两种真实物理形态，并保证 semantic gap 为零。
- `lba4_server_flag_wire_rule_is_restore_profile_specific_in_real_fixtures` 固定真实盘
  current/legacy 两种 restore profile：current profile 使用官方 post-XOR physical
  flags；legacy profile 保留 generic ReadSector4 rolling 语义，防止再次把任一单一
  解码规则强套到全部历史盘。
- `validator_rejects_historical_lba4_short_form_as_new_media_canonical` 固定“历史 short
  form 可读、但新盘 current SAFE6 必须 full rolling”的生成/兼容边界。
- `lba7_post_table_plaintext_is_zero_through_sector_end` 固定 packed LBA7 `+0x0CE..+0x1FF`
  为 writer-owned zero region，防止把306B再次退回 UNKNOWN 或误作可携带 payload。

本轮已经修改 `src/inspect.rs`、`src/provision/generate.rs`、
`src/provision/validate.rs`：inspect 现在按 restore profile 区分 current post-XOR
flags 与 legacy rolling-reader flags；Provision 使用 current SAFE6 full rolling +
两字节覆盖；validator 精确重建并校验 current wire profile。后续改动这些路径仍
必须同时运行 inspect / provision_generate / provision_validate / golden，不能只跑协议审计。

整个分析过程未对真实物理 USB 执行任何 raw write。

未经用户再次明确指定某块物理测试 U 盘，不得对真实 raw disk 执行 `provision write`；优先用 Linux loop / Windows VHD HIL 验证写入和 rollback。
