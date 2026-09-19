# 新 U 盘 Provisioning 交接（2026-09-19）

仓库：`Evolution404/edpcli`

分支：`feat/provision-new-usb-20260919`

基线：已正式发布的 `v2.2.0`，`main@14557e7e54e355c5853479a7220e8ff1f0e5e9e7`。

最新协议审计内容基线提交：
`95192a6 audit: close LBA7 legacy wrapped keys`。
交接文档更新提交应位于其后；接手时只要求当前分支已包含该提交、与 origin 同步且工作区 clean。

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

截至最新逐字节审计，严格统计为：

- **COMPLETE：1627 / 6656B = 24.4%**
- **PARTIAL：3004 / 6656B = 45.1%**
- **UNKNOWN：2025 / 6656B = 30.4%**

当前各 LBA 严格状态以主账本为唯一准绳，最新关键增量：

- **LBA7 = 179 COMPLETE / 27 PARTIAL / 306 UNKNOWN = 35.0%**。
  真实物理 LBA7 已锁定为 **3×0x40 packed EDPF + 14B pass-info@+0xC0**，
  不能用 `libcemsfilesyscheck.so` 的 0x48/72B natural ABI 直接解释盘面。
  v0x0064 legacy wrapped key `entry+0x38..0x3F` 已完整闭合：
  `fold32("0000aaaa")=0x91919191`，两个 DWORD 分别 XOR 解包；
  22份 original real-device 中全部28条非零 type2/type4 legacy entry **28/28**
  通过 `CRC32_bare(file_key8)==FileKeyCRC`。producer/consumer/写回链为
  `ChangePwd -> sub_10026050 -> sub_10028DB0 -> sub_100125B0
  -> SavePartionSector/sub_10028580 -> sub_10010FC0 -> WriteFile(LBA7)`。
- **LBA12 = 393 COMPLETE / 119 PARTIAL / 0 UNKNOWN = 76.8%**。
  主运行时盘面固定为 3×96B packed entry；`Reserved[7]@+0x59..+0x5F`
  已由官方字段名、writer 零来源、negative consumer 和 66/66 原始 entry 闭合。
  v0x0206 默认密码 mode2 wrapping 也已独立闭合：
  `"0000aaaa" -> sub_10040400 -> "LtSWi[2f)j"`，
  MD5 后走标准 SM4；43/43 默认 mode2 原始 entry 可独立解包并通过 FileKeyCRC。
  但 mode1/mode3/oldSM4 等已知 profile 缺正向原盘，不得把整个 wrapped16 升 COMPLETE。
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
  `UserLogin` 分别把两者传给 `SetVolumeLabelA`；唯一启用 EESI 的原始
  SanDisk 实盘已作为最小512B证据夹具加入 CI。
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
- pass-info 当前只剩3B语义未闭合：
  `+0x0A bNoUsbChkPasSafe`、`+0x0C ShareBackuppromptPeriod`、
  `+0x0D EncryptBackuppromptPeriod`。其中 `+0x0A` 22盘为18×0/4×1，
  LBA7/LBA12 逐盘一致；Windows Init 已向 EXE 输出，但当前 EXE 没找到最终策略 consumer。

下一位 AI 优先顺序：

1. **先检查 git 状态并读主账本，不要重复已完成分析。**
   当前 HEAD 必须包含 `95192a6` 或更新提交，branch 与 origin 同步且 worktree clean。
   必须先读：
   `AGENTS.md`、
   `docs/HANDOFF_PROVISION_NEW_USB_2026-09-19.md`、
   `docs/PROTOCOL_BYTE_TRACE_2026-09-19.md`、
   `docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md`。
   禁止 `reset/clean`。
2. **首攻 LBA7 剩余 27B PARTIAL**：
   - 逐 entry 追 `Version@+0x04` 的 producer/consumer/version-switch；
   - entry1/entry2 `NeedDisturb@+0x10` 当前没有 direct xref，继续搜其它组件/历史 build；
   - pass-info `+0x0A/+0x0C/+0x0D` 继续追跨组件最终 consumer。
   不要再重复分析 wrapped8；其 24B 已 COMPLETE。
3. **LBA4 legacy HSerialCRC[5] producer**：
   当前 writer machine code 已闭合，但旧 14/22 固定
   `1D29,7B,4DD,79,7C` 与2份高熵 profile 的生成源仍未知。
   `edpuniqueid` 的 `Drive%dSerialNumber`、`DeviceNumber.dll::EDP_DiskNumber`
   已查过，当前没有建立到五槽 writer 的证据链；不要重复把它们硬接。
4. **LBA11 rev_pmap / CHS profile**：
   当前 LBA11 260B COMPLETE、252B PARTIAL；21份使用 DiskSize，1份 Aigo U335
   `rev_pmap` 使用 CHS 容量。若能闭合“何时选择 CHS”上游条件，潜在可一次提升252B。
5. **LBA6 legacy +0x1E0..+0x1EF**：
   20/22零、2份旧格式非零；current template 为零，但旧 producer/consumer未知。
   GSerial/BeiZhu 的 post-NUL 残值已明确是 backing bytes，不要再按 padding。
6. **LBA8 ELABEL 与动态头剩余字段**：
   static ToolVersion/Labversion/writeTime/Reserved 已 COMPLETE；
   继续为 HDSerialInfo/MacInfo/UsbOnlyInfo 与17-key ELABEL 每个业务字段追最终 consumer。
7. **LBA10 `+0x04` 与 `+0x28..`**：`+0x04` 目前只有“默认/实盘=1、API原样
   读写”，没有业务分支 consumer，继续保持 PARTIAL；不要沿用旧文档“版本1/时间戳”
   猜测。两个16B卷标槽已经 COMPLETE，不要重复追。
8. **LBA0 bootstrap / LBA1-LBA2 GPT 正样本**：LBA0分区表+55AA及3B legacy
   message pointer 已闭合，
   bootstrap 446B仍 PARTIAL。若能找到真实原始 GPT EDP 盘，可用于把 LBA1/LBA2
   从 PARTIAL 继续细分；在此之前不得以 synthetic builder 输出冒充实盘证据。
9. **LBA3 MP payload**：当前已明确 EDP 只 preserve/ignore；若继续追，目标应是
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

最新 LBA7 legacy wrapped-key 收口后实际验证：

- `cargo test --test provision_protocol_audit --locked`：**34/34 PASS**；
- `cargo test --test protocol_documentation_contract --locked`：**3/3 PASS**；
- 新增门禁：
  `lba7_physical_entries_are_packed_64_not_linux_natural_72` PASS；
- 新增门禁：
  `lba7_v64_packed_legacy_file_key_wrap_matches_real_fixtures` PASS；
- `git diff --check`：PASS。

更早阶段的 inspect / provision_generate / provision_validate / golden 已在各自收口时通过，
但本次纯协议审计批次没有重新把它们作为必跑项；下一位若改动生成/inspect 路径，必须重新跑对应测试。

整个分析过程未对真实物理 USB 执行任何 raw write。

未经用户再次明确指定某块物理测试 U 盘，不得对真实 raw disk 执行 `provision write`；优先用 Linux loop / Windows VHD HIL 验证写入和 rollback。
