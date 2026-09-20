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

截至本次 LBA11 repair-profile 闭环，严格统计为：

- **COMPLETE：2883 / 6656B = 43.3%**
- **PARTIAL：3773 / 6656B = 56.7%**
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
- **LBA9 = 54 COMPLETE / 458 PARTIAL / 0 UNKNOWN = 10.5%**。
  EPPE writer-zero tail、SAPF trailing/backing 等已无UNKNOWN。
  本轮进一步纠正中间区：Windows/Linux/vrvaud 三套 BuildSector6 都会把
  长 Dept `[60..NUL]` 写到 LBA9+0x80、长 User `[28..NUL]` 写到
  LBA9+0x100，所以这里不是纯 preserve 区。严格22盘有8份 long-Dept：
  4份 current join=60、4份 legacy join=59；官方 reader 两种接缝都支持，
  两组均重建为同一76B合法GBK Dept。join59 的旧 producer仍未找到，
  因此 +0x80..0xFF 暂不升COMPLETE；+0x100..0x17F 又与 SAPF/long-User
  profile重叠且22盘没有长User正向样本，继续PARTIAL。
- **LBA10 = 424 COMPLETE / 88 PARTIAL / 0 UNKNOWN = 82.8%**。
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

- **LBA6 = 300 COMPLETE / 212 PARTIAL / 0 UNKNOWN = 58.6%**。
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
  因此 `m_autoid[16]@+0x70` 和 `m_UsbOffice[64]@+0x80` 的 post-NUL
  非零字节是明确的 writer-uninitialized backing，而不是未知隐藏字段。
  ReadSector6 只按 C-string 消费，前508B checksum 又覆盖全部物理字节；
  committed originals 中同一空 autoid 至少2种尾、同一空 Office 至少3种尾。
  这80B现已升级 COMPLETE。Owner 仍受 long-User 未见正向实盘影响，
  Label 仍有56B盘面截断/profile缺口，继续PARTIAL。
  Dept/Owner 长值的 continuation 还确认落在 LBA9+0x80/+0x100，
  并已用 join60/join59 双profile实盘门禁锁定。当前全 LBA0–12 已无 UNKNOWN，
  目前全局仍有3773B PARTIAL，绝不能把“UNKNOWN=0”当成全部协议完成。

- **LBA7 = 490 COMPLETE / 22 PARTIAL / 0 UNKNOWN = 95.7%**。
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
  因此 entry0 `NeedDisturb` 可明确描述为 **MBR scramble/descramble gate**；
  entry1/entry2 同名字段仍没有独立 consumer，8B继续PARTIAL。
  最新又从独立 Linux `checkdiskback` 找到 pass-info
  `bNoUsbChkPasSafe(+0x0A)` 的真实 consumer：
  `Update_EDPEDISKSHOWPARAM@0x406B70` 执行
  `pass+0x0A == 1 ? showparam+3=0 : showparam+3=1`；
  `CreateSafe6TmpPolicyFile` 随后将其写入 CRC/A6B0 加密策略文件，
  `EdpEDiskBack` 与 `linuxedpedisk` 两套 `Safe6PolicyFile` 独立恢复。
  22份原始盘为18×0+4×1，且22/22 LBA7/LBA12一致，因此 LBA7/LBA12
  各1B升级COMPLETE。LBA7现在只剩 **22B PARTIAL**：
  3×Version(12B)、entry1/2 NeedDisturb(8B)、BackupPromptPeriod(2B)。
- **LBA8 = 92 COMPLETE / 420 PARTIAL / 0 UNKNOWN = 18.0%**。
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
  UsbOnlyInfo为空并保留历史非零 HDSerialInfo。进一步把42B混合区拆开后，
  `MacInfo[6]@+0x18..+0x1D` 已满足独立闭合条件：官方字段名明确、
  Windows/Linux header 都显式零初始化、semantic reader 完全跳过，
  且 current/legacy 22/22 均为6B零，无已知 profile 分叉，因此6B升COMPLETE。
  `HDSerialInfo@+0x14..17` 与 `UsbOnlyInfo@+0x1E..3D` 仍因 legacy producer/
  历史 consumer 缺口保持PARTIAL。
- **LBA12 = 394 COMPLETE / 118 PARTIAL / 0 UNKNOWN = 77.0%**。
  主运行时盘面固定为 3×96B packed entry；`Reserved[7]@+0x59..+0x5F`
  已由官方字段名、writer 零来源、negative consumer 和 66/66 原始 entry 闭合。
  v0x0206 默认密码 mode2 wrapping 也已独立闭合：
  `"0000aaaa" -> sub_10040400 -> "LtSWi[2f)j"`，
  MD5 后走标准 SM4；43/43 默认 mode2 原始 entry 可独立解包并通过 FileKeyCRC。
  但 mode1/mode3/oldSM4 等已知 profile 缺正向原盘，不得把整个 wrapped16 升 COMPLETE。
- **LBA11 = 512 COMPLETE / 0 PARTIAL / 0 UNKNOWN = 100%**。
  正常注册 writer/reader 使用 `DISK_GEOMETRY_EX.DiskSize`；
  `UDiskLabelRepair.dll::CLabelRepair::Repair` 的 LBA11 检查/重写路径使用
  `IOCTL_DISK_GET_DRIVE_GEOMETRY` 得到传统 `DISK_GEOMETRY`，按
  `Cylinders*TracksPerCylinder*SectorsPerTrack*BytesPerSector` 计算 CHS capacity。
  `sub_10008820 -> sub_10003BD0` 用该容量校验 LBA11，失败后
  `sub_10008950(ReWrite11Sector) -> sub_10003A40` 用同一容量重建 LBA11；同一
  Aigo U335 rev_pmap 的 CHS/exact 双真实捕获与两条官方路径吻合。因此后半252B已
  从 PARTIAL 升 COMPLETE，不再重复追 CHS profile。
- **LBA0 = 69 COMPLETE / 443 PARTIAL**。
  legacy MBR `+0x1B5..+0x1B7 = 2C 44 63` 已闭合为三个错误消息指针低字节；
  22盘只有 template/cleared 两种 profile。其余 bootstrap/profile 仍未全部解释。
- **LBA3 = 整扇 PARTIAL**。
  EDP 注册链只 preserve existing，当前 EDP reader 不解析；22盘 21零 + 1份
  Kingston `this is mp mark\0` 制造 payload。厂商 MP producer/固件 consumer 未找到。
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
  **LBA10 当前为 424 COMPLETE / 88 PARTIAL / 0 UNKNOWN = 82.8%**：
  `+0x04..07` 已由两套独立 `EdpEDisk.exe` 的启动写1 / 标签设置保存写0 producer，
  `UsbSuspensionWnd.dll` 的 Destroy 与刷新/Show 值相关 consumer，以及原始 SanDisk
  实盘=1闭合为 COMPLETE；
  后384B已按 cross-generation unowned preserve/ignore 语义闭合。两代独立
  EESI setter 都只替换前0x80并原样写回 tail，getter完全不暴露 tail；
  两个更老 build 连 EESI 路径都不存在。committed originals + 独立 SanDisk
  继续锁定真实样本，额外58份经 LBA6 CRC + LBA12 EDPF 双重验证的历史快照
  也为58/58 tail零。COMPLETE 不代表 tail 必须为零，非零旧盘仍必须原样 preserve。
  当前只剩 EESI `+0x28..7F` 88B PARTIAL。两套卷标设置 `IDOK` handler 都明确
  先清零完整0x80B再只填两个卷标，所以 current UI producer 对这88B写零；但底层
  Set API 会完整 round-trip 调用者输入，仍缺正式字段划分、非零 profile 与值相关 consumer。
  新增补充实盘：2026-08-04 Netac OnlyDisk 历史6656B捕获在独立
  `device_id=disk&ven_netac&prod_onlydisk&rev_0000` 下通过 LBA6 CRC、LBA7/LBA12
  EDPF 自洽校验，LBA10 同样解出 `EESI/+0x04=1/交换区/保密区` 且后88B全零；
  已加测试门禁，但在来源链完全审计前不并入22份严格生成参考，也不提升COMPLETE。
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
  `v0x0206 + mode2 + oldSM4!="1"` 已闭合，但 mode1/mode3/oldSM4
  没有正向原盘，所以整个16B字段仍按严格规则保持 PARTIAL。
- LBA12 `+0x48..+0x57` 是扩展材料槽；当前66/66原始 entry 为零，但历史用途未闭合。
  相邻 `+0x59..+0x5F` 才是已经 COMPLETE 的 `Reserved[7]`，不要混成一片。
- pass-info 当前只剩2B语义未闭合：
  `+0x0C ShareBackuppromptPeriod`、`+0x0D EncryptBackuppromptPeriod`。
  `+0x0A bNoUsbChkPasSafe` 已由独立 `checkdiskback`
  `Update_EDPEDISKSHOWPARAM` 的值相关策略映射和两套 Safe6PolicyFile 消费链闭合。

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
3. **LBA7 剩余 22B PARTIAL**：
   - 三条 `Version@+0x04`：12B，继续追 producer/consumer/version-switch；
   - entry1/entry2 `NeedDisturb@+0x10`：8B，当前没有 direct xref，继续搜其它组件/历史 build；
   - pass-info `+0x0C/+0x0D`：2B，继续追跨组件最终 consumer。
   本轮已重新核实两版 `vrvaud_c` 全局 old-table 都是3×0x40 packed，且只有
   entry0 NeedDisturb 有行为 xref；Version、entry1/2 NeedDisturb 在两版均无直接
   consumer。该负证据不能把正式ABI字段升级成 Reserved/COMPLETE。
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
   继续为 HDSerialInfo/MacInfo/UsbOnlyInfo 与17-key ELABEL 每个业务字段追最终 consumer。
7. **LBA10 只剩 `+0x28..+0x7F` 88B**：`+0x04` 已闭合为
   `UsbSuspensionWnd lifecycle/control flag`，不要回退到“版本1/时间戳”或PARTIAL。
   两个16B卷标槽也已 COMPLETE。当前88B已有两套官方 UI zero producer、底层
   Get/Set opaque round-trip，以及 SanDisk + 补充 Netac 两个 EESI 零 profile；
   继续目标是正式字段声明、非零历史 profile 或值相关 consumer，缺任一关键环节前
   不得把这88B机械升级为 reserved/COMPLETE。
8. **LBA0 bootstrap / LBA1-LBA2 GPT 正样本**：LBA0分区表+55AA及3B legacy
   message pointer 已闭合，
   bootstrap 446B仍 PARTIAL。若能找到真实原始 GPT EDP 盘，可用于把 LBA1/LBA2
   从 PARTIAL 继续细分；在此之前不得以 synthetic builder 输出冒充实盘证据。
10. **LBA3 MP payload**：当前已明确 EDP 只 preserve/ignore；若继续追，目标应是
   真正厂商 MP producer/firmware consumer，而不是再证明 EDP 不使用它。

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
