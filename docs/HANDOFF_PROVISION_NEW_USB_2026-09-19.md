# 新 U 盘 Provisioning 交接（2026-09-19）

仓库：`Evolution404/edpcli`

分支：`feat/provision-new-usb-20260919`

基线：已正式发布的 `v2.2.0`，`main@14557e7e54e355c5853479a7220e8ff1f0e5e9e7`。

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
- `+0x10 NeedDisturb`：字段名、写端来源、标准三分区写值已闭合：
  `Boot=1, Share=1, Encrypt=0`；但 Windows 用户态→EdpMountFile→驱动链未发现
  该字段进入运行时参数，因此**真实行为仍未闭合，禁止按名字猜语义**。
- `+0x38..0x47` 是 16B wrapped file-key material；`+0x48..0x57` 当前主 writer
  不写、主 reader 不读、历史样本全零，只能定性为未使用扩展槽，不能宣称协议恒零。
- Linux `SetPartitionNewPass` 只改 `UserKeyCRC(+0x30)` 和 wrapped key
  `(+0x38)`，不会触碰 `+0x48..0x57/+0x58/+0x5c..0x5f`。
- pass-info 14B 已确认两组密码失败次数：
  `+0x03/+0x04` = Share max/current，
  `+0x06/+0x07` = Encrypt max/current。
  改密成功会同步清 `+0x02/+0x04` 或 `+0x05/+0x07`，
  所以 `+0x02/+0x05` 属于对应密码状态组，但准确语义仍需追。

下一位 AI 优先顺序：
1. LBA6 当前 writer 边界已闭合：`0x1C0..0x1CF=GSerial`、
   `0x1D0..0x1DF=BeiZhu`、`0x1E0..0x1EF=模板/旧版扩展`、
   `0x1F0..0x1F3=m_encrypt`；继续只追 2/22 旧格式非零扩展来源。
   `+0x1CA` 已证实位于 GSerial 槽内，不得再当独立状态字段。
2. 继续追 LBA8 LLGB/EKTF 的动态字段和 writer 来源；
3. 继续追 LBA12 `NeedDisturb(+0x10)` 的旧版正向消费者，以及 pass-info
   `+0x0A/+0x0C/+0x0D` 的跨组件消费；
4. LBA4 `OnllyID2Nd/HSerialCRC[5]` 已恢复官方结构；继续追非当前 writer
   profile 的 HSerialCRC 上游；
5. 回到 LBA0/LBA3/LBA7/LBA11 的剩余非 canonical / 代际差异。

每得到一批闭合结论，都要同时更新
`docs/PROVISION_PROTOCOL_AUDIT_2026-09-19.md` 和
`tests/provision_protocol_audit.rs`，并单独小 commit + push。

未经用户再次明确指定某块物理测试 U 盘，不得对真实 raw disk 执行 `provision write`；优先用 Linux loop / Windows VHD HIL 验证写入和 rollback。
