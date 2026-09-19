# 新 U 盘 Provisioning 交接（2026-09-19）

仓库：`Evolution404/edpcli`

分支：`feat/provision-new-usb-20260919`

基线：已正式发布的 `v2.1.0`，`main@97600fd175ff2151b9cd6d2757741163148e4368`。

目标：让一块普通全新 USB 能生成并安全写入 EDP/cems 前部 metadata。审计/备份范围保留 LBA0–13（14 sectors），但最新官方写路径证据显示 `RegsiterUsb` 只连续写 13 sectors，即 **LBA0–12**；LBA13 默认保留原值。第一阶段不格式化数据区，不写 LBA13+。

先完整阅读 `docs/PROVISION_NEW_USB_PLAN_2026-09-19.md`、`docs/RELEASE.md`，再检查 git 状态。允许连接 Mac，Phase 0 优先用现有备份和真实盘做**只读**协议审计。不要把现有 `apply` 直接改造成 provision；必须先建立纯 `ProvisionSpec/Profile/Image/Validator`。

严格 Phase 0→7，测试先行、小 commit、及时 push。最新 Phase 0 结论包括：onlyid 官方链为 `CoCreateGuid -> CRC32_bare(raw16)`；LBA11 为 `DRKB + random252`；LBA12 是整扇连续 A6B0/A7F0；LBA13 不属于当前已证明的官方写集。其余未知 reserved/dynamic bytes 不得猜测或随意清零。CLI/TUI 必须复用 application/service 和现有写盘安全链。

未经用户再次明确指定某块物理测试 U 盘，不得对真实 raw disk 执行 `provision write`；优先用 Linux loop / Windows VHD HIL 验证写入和 rollback。
