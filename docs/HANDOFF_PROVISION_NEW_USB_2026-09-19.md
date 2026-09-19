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

截至本次 LBA4 最新审计，严格统计为：

- **COMPLETE：1629 / 6656B = 24.5%**
- **PARTIAL：4267 / 6656B = 64.1%**
- **UNKNOWN：760 / 6656B = 11.4%**

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
  `bDataToServer/bConnetServer`。22份原始盘 22/22 physical flags 与 generic rolling
  输出不同，证明旧 inspect 解码错误。现已改为 inspect 恢复 physical post-XOR flags，
  Provision 使用官方 current SAFE6 full rolling + 两字节 post-XOR覆盖，validator
  精确校验 full wire profile；历史 raw-zero short form 仅保留兼容读取。
  两flag仍因缺最终业务consumer保持PARTIAL，严格完成字节数不增加。
- **LBA9 = 54 COMPLETE / 458 PARTIAL / 0 UNKNOWN = 10.5%**。
  EPPE writer-zero tail、历史 Dept/backing、SAPF trailing/backing 与 post-SAPF
  preserve 区已经全部从 UNKNOWN 降到 PARTIAL；整扇不再有 UNKNOWN。
- **LBA10 = 36 COMPLETE / 476 PARTIAL / 0 UNKNOWN = 7.0%**。
  EESI 前0x80 round-trip payload 与后0x180 current preserve/ignore storage boundary
  已闭合到 PARTIAL；整扇不再有 UNKNOWN。

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
3. **LBA7 剩余 27B PARTIAL**：
   - 逐 entry 追 `Version@+0x04` 的 producer/consumer/version-switch；
   - entry1/entry2 `NeedDisturb@+0x10` 当前没有 direct xref，继续搜其它组件/历史 build；
   - pass-info `+0x0A/+0x0C/+0x0D` 继续追跨组件最终 consumer。
   不要再重复分析 wrapped8；其 24B 已 COMPLETE。
4. **LBA4 legacy HSerialCRC[5] producer**：
   当前 writer machine code 已闭合，但旧 14/22 固定
   `1D29,7B,4DD,79,7C` 与2份高熵 profile 的生成源仍未知。
   `edpuniqueid` 的 `Drive%dSerialNumber`、`DeviceNumber.dll::EDP_DiskNumber`
   已查过，当前没有建立到五槽 writer 的证据链；不要重复把它们硬接。
5. **LBA11 rev_pmap / CHS profile**：
   当前 LBA11 260B COMPLETE、252B PARTIAL；21份使用 DiskSize，1份 Aigo U335
   `rev_pmap` 使用 CHS 容量。若能闭合“何时选择 CHS”上游条件，潜在可一次提升252B。
6. **LBA6 legacy +0x1E0..+0x1EF**：
   20/22零、2份旧格式非零；current template 为零，但旧 producer/consumer未知。
   GSerial/BeiZhu 的 post-NUL 残值已明确是 backing bytes，不要再按 padding。
7. **LBA8 ELABEL 与动态头剩余字段**：
   static ToolVersion/Labversion/writeTime/Reserved 已 COMPLETE；
   继续为 HDSerialInfo/MacInfo/UsbOnlyInfo 与17-key ELABEL 每个业务字段追最终 consumer。
8. **LBA10 `+0x04` 与 `+0x28..`**：`+0x04` 目前只有“默认/实盘=1、API原样
   读写”，没有业务分支 consumer，继续保持 PARTIAL；不要沿用旧文档“版本1/时间戳”
   猜测。两个16B卷标槽已经 COMPLETE，不要重复追。
9. **LBA0 bootstrap / LBA1-LBA2 GPT 正样本**：LBA0分区表+55AA及3B legacy
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

- `cargo test --test inspect --locked`：**19/19 PASS**；
- `cargo test --test provision_generate --locked`：**3/3 PASS**；
- `cargo test --test provision_validate --locked`：**6/6 PASS**；
- `cargo test --test provision_protocol_audit --locked`：**47/47 PASS**；
- `cargo test --test protocol_documentation_contract --locked`：**3/3 PASS**；
- `cargo test --test golden --locked`：**14/14 PASS**；
- `git diff --check`：PASS。

本轮强化的 LBA4 门禁：

- `lba4_short_form_must_be_decoded_by_regions_not_by_zero_bytes` 必须同时覆盖
  raw-zero 与 rolling-encrypted-zero 两种真实物理形态，并保证 semantic gap 为零。
- `lba4_server_flags_are_post_xor_wire_bytes_in_real_fixtures` 固定真实盘中 physical
  post-XOR flags 与 generic ReadSector4 rolling 输出的分叉，防止再次把后者误当字段值。
- `validator_rejects_historical_lba4_short_form_as_new_media_canonical` 固定“历史 short
  form 可读、但新盘 current SAFE6 必须 full rolling”的生成/兼容边界。

本轮已经修改 `src/inspect.rs`、`src/provision/generate.rs`、
`src/provision/validate.rs`：inspect 恢复 producer-side post-XOR flags，Provision
切换到 current SAFE6 full rolling + 两字节覆盖，validator 精确重建并校验 current
wire profile。后续改动这些路径仍必须同时运行 inspect / provision_generate /
provision_validate / golden，不能只跑协议审计。

整个分析过程未对真实物理 USB 执行任何 raw write。

未经用户再次明确指定某块物理测试 U 盘，不得对真实 raw disk 执行 `provision write`；优先用 Linux loop / Windows VHD HIL 验证写入和 rollback。
