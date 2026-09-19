# 新 U 盘 Provisioning 交接（2026-09-19）

仓库：`Evolution404/edpcli`

分支：`feat/provision-new-usb-20260919`

基线：已正式发布的 `v2.1.0`，`main@97600fd175ff2151b9cd6d2757741163148e4368`。

目标：让一块普通全新 USB 能生成并安全写入完整 LBA0–13 EDP/cems metadata；第一阶段不格式化数据区，不写 LBA14+。

先完整阅读 `docs/PROVISION_NEW_USB_PLAN_2026-09-19.md`、`docs/RELEASE.md`，再检查 git 状态。允许连接 Mac，Phase 0 优先用现有备份和真实盘做**只读**协议审计。不要把现有 `apply` 直接改造成 provision；必须先建立纯 `ProvisionSpec/Profile/Image/Validator`。

严格 Phase 0→7，测试先行、小 commit、及时 push。未知 reserved bytes、onlyid 算法、LBA12 tail 等不得猜测或随意清零。CLI/TUI 必须复用 application/service 和现有写盘安全链。

未经用户再次明确指定某块物理测试 U 盘，不得对真实 raw disk 执行 `provision write`；优先用 Linux loop / Windows VHD HIL 验证写入和 rollback。
