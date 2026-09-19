# 新 U 盘 Provisioning 交接（2026-09-19）

仓库：`Evolution404/edpcli`

分支：`feat/provision-new-usb-20260919`

基线：已正式发布的 `v2.2.0`，`main@14557e7e54e355c5853479a7220e8ff1f0e5e9e7`。

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

- **COMPLETE：1535 / 6656B = 23.1%**
- **PARTIAL：2584 / 6656B = 38.8%**
- **UNKNOWN：2537 / 6656B = 38.1%**

其中最近几轮新增的高置信结论：

- LBA9 EETU：`ullBTime`、`ullETime`、`useCount` 共20B已由
  Windows producer + Linux `CheckTempUse` consumer + 20份真实EETU实盘闭合；
  `0xFFFFFFFF` 明确是无限次数哨兵。
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

LBA12 必须继续按“**结构已知 != 语义已知**”的严格口径推进：

- 主运行时盘面固定为 `96B * 3 EDPF entries + 14B pass-info`；不要与
  `libcemsfilesyscheck.so` 中 104B 扩展结构混用。
- `+0x14 NeedEncrypt` 已闭合：Windows `InitDiskInfo/UserLogin` 实际消费，
  `0=不启用透明加密，1=启用透明加密`。
- `+0x58 EncryptMode` 已闭合为 **1 byte**：
  `0=AES64, 1=AES128, 2=SMS4, 3=AESOPENSSL`。Linux 主挂载路径当前只创建
  mode 0/1/2 header；Windows 对 mode 3 有兼容回退。
- `+0x10 NeedDisturb`：字段名、写端来源和旧版兼容 consumer 已闭合到
  **entry0 的兼容门控行为**：`NewCheckDisTurbUsb(*)` fallback 直接以
  `entry0+0x10 != 0` 判定 success；新版主路径和其它 entry 的进一步业务作用
  仍未闭合，禁止按名字翻译成“扰码/激活/防篡改”等更具体语义。
- `+0x38..0x47` 是 16B wrapped file-key material；`+0x48..0x57` 当前主 writer
  不写、主 reader 不读、历史样本全零，只能定性为未使用扩展槽，不能宣称协议恒零。
- Linux `SetPartitionNewPass` 只改 `UserKeyCRC(+0x30)` 和 wrapped key
  `(+0x38)`，不会触碰 `+0x48..0x57/+0x58/+0x5c..0x5f`。
- pass-info 14B 已确认两组密码失败次数：
  `+0x03/+0x04` = Share max/current，
  `+0x06/+0x07` = Encrypt max/current。
  `+0x02/+0x05` 已由登录/改密路径闭合为 Share/Encrypt 强制改密标志；
  `+0x0B bResetFileKey` 已闭合为“强制改密时是否重新生成16B file-key材料”的门控。
  当前真正未闭合的是 `+0x0A bNoUsbChkPasSafe` 的最终策略消费者，
  以及 `+0x0C/+0x0D Share/EncryptBackuppromptPeriod` 的实际消费语义。

下一位 AI 优先顺序：

1. **先检查 git 状态并读主账本，不要重复已完成分析。**
   当前 LBA1/LBA2 GPT profile 修改已经通过：
   `inspect 17/17`、`provision_protocol_audit 24/24`、
   `protocol_documentation_contract 3/3`。
   若交接时仍未提交，先复核 diff 后小 commit + push；不得 reset/clean。
2. **LBA3**：当前22份中21份全零、1份 Kingston 含 `this is mp mark`。
   优先追制造标记 producer/consumer，判断它是厂商 MP 标记、注册流程保留区，
   还是可完全从协议排除；不能因大多数样本为零直接 COMPLETE。
3. **LBA6 旧 profile**：当前 writer 边界已闭合：`0x1C0..0x1CF=GSerial`、
   `0x1D0..0x1DF=BeiZhu`、`0x1E0..0x1EF=模板/旧版扩展`、
   `0x1F0..0x1F3=m_encrypt`；继续只追 2/22 旧格式非零扩展来源。
   `+0x1CA` 已证实位于 GSerial 槽内，不得再当独立状态字段。
4. **LBA8 动态头**：当前原始参考集已确认 22/22 LLGB；Windows/Linux 都使用同一 17-key
   ELABEL writer 模板，`+0x04` 不含结尾 NUL，加密长度为
   `((logical_len / 16) + 1) * 16`。edpcli inspect / Provision 已去掉固定
   0x170 假设。继续只追 LLGB 动态头 `+0x10/+0x14/+0x18..` 的生成源；
5. **LBA7/LBA12 剩余 EDPF / pass-info**：LBA12 实现已统一为整扇 512B 连续 A6B0：inspect / 旧盘 convert /
   Provision builder / validator 均已去掉“368B 密文 + 144B RAW”的实现；
   `0x170` 仅保留为 `EDPF_TABLE_LEN`。旧盘转换 golden 14/14 哈希不变。
   `NeedDisturb(+0x10)` 已找到旧版正向消费者：
   `NewCheckDisTurbUsb(*)` fallback 直接以 entry0 +0x10 非零作为 success 门控，
   且两套 Windows 构建独立一致；canonical Share/entry0 必须保持非零。
   pass-info 剩余项也已进一步收窄：`+0x0A` 在 22 份原始样本中
   18×0 / 4×1，LBA7/LBA12 逐样本一致，并由 Windows
   `CEdpEDiskCtrlInterface::Init` 导出到输出结构 `+0x11`；但最终策略消费者未闭合。
   `+0x0C/+0x0D` 22/22 为零，Windows/Linux 当前组件均未找到直接消费者，
   只能保留官方字段名，不能当 padding。下一步可回到 LBA4 非当前 writer profile
   的 HSerialCRC 上游，或继续追 NeedDisturb 新版主路径；
6. **LBA4 legacy profile**：`OnllyID2Nd/HSerialCRC[5]` 已恢复官方结构。Provision 已修正为当前
   Windows writer profile：`OnlyIdXor8=onlyid^0x88888888`、
   `OnllyID2Nd=onlyid`、`HSerialCRC[5]=0`，并删除旧
   `lba4_nonce` / sparse-XOR 设计。22 份样本的旧 HSerial profile 与
   LBA9/LBA6 形态有强相关，但只能记相关性；`DeviceNumber.dll::EDP_DiskNumber`
   只返回单 DWORD，当前没有连接到标签 writer，不得把它直接当成
   HSerialCRC[5] 来源。旧 profile 的 5×DWORD 上游仍未闭合；
7. **LBA10 `+0x04` 与 `+0x28..`**：`+0x04` 目前只有“默认/实盘=1、API原样
   读写”，没有业务分支 consumer，继续保持 PARTIAL；不要沿用旧文档“版本1/时间戳”
   猜测。两个16B卷标槽已经 COMPLETE，不要重复追。
8. **LBA0 bootstrap / LBA1-LBA2 GPT 正样本**：LBA0分区表+55AA已闭合，
   bootstrap 446B仍 PARTIAL。若能找到真实原始 GPT EDP 盘，可用于把 LBA1/LBA2
   从 PARTIAL 继续细分；在此之前不得以 synthetic builder 输出冒充实盘证据。

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

本轮 LBA1/LBA2 GPT profile 收口后已通过：

- `cargo test --test inspect --locked`：17/17 PASS；
- `cargo test --test provision_protocol_audit --locked`：24/24 PASS；
- `cargo test --test protocol_documentation_contract --locked`：3/3 PASS；
- `cargo test --test provision_generate --locked`：3/3 PASS；
- `cargo test --test provision_validate --locked`：5/5 PASS；
- `cargo test --test golden --locked`：14/14 PASS；
- `git diff --check`：PASS。

整个分析过程未对真实物理 USB 执行任何 raw write。

未经用户再次明确指定某块物理测试 U 盘，不得对真实 raw disk 执行 `provision write`；优先用 Linux loop / Windows VHD HIL 验证写入和 rollback。
