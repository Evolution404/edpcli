# edpcli 版本与发布规范

本文档是后续维护和 AI 继续开发时的版本号唯一决策基线。除非用户再次明确要求重置版本线，
不得自行跳号、回退、复用已经发布过的版本号或重新解释版本规则。

## 当前版本线

- 当前待发布版本：**2.1.0**。
- 2.1.0 为向后兼容的 MINOR 发布：新增基于 ratatui + crossterm 的跨平台 Vim 风格 TUI；CLI v2 命令与脚本行为保持兼容。
- TUI 与 CLI 共用 application/service 和写盘安全链；新增 Device dashboard、Backup workspace、只读 backup create、Apply/Restore 安全向导、Inspect/hex、搜索与 command palette。

- 正式版本线从 **1.0.0** 开始。
- `Cargo.toml` 中的 `package.version` 是源代码版本的唯一事实源。
- 正式 tag 必须使用 `vMAJOR.MINOR.PATCH`，并且必须与 `Cargo.toml` 完全一致。
- GitHub Release workflow 会校验 tag 与 `Cargo.toml`；不一致时拒绝发布。

2026-09-18 经用户明确要求，项目版本线从此前的 `3.0.0` 重新起算为 `1.0.0`。这是一次
明确授权的版本线重置，不应被后续 AI 当作一般惯例重复执行。

2026-09-18 起，正式发布架构矩阵固定为：macOS arm64、macOS x86_64、macOS Universal、
Linux arm64、Linux x86_64、Windows arm64、Windows x86_64。新增/删除正式架构属于对公开
发布能力的变更，按 SemVer 决定版本级别；当前首次加入 Linux/Windows arm64 使用 `1.1.0`。

## 后续版本号由维护 AI 自主决定

以后每次正式发布，维护 AI 根据本次变更自行选择 SemVer 版本，不需要为了普通版本升级再次
询问用户。决策遵循以下顺序：

### PATCH：`1.0.0 -> 1.0.1`

适用于保持现有外部行为兼容的修复，例如：

- Bug 修复；
- 安全加固但不改变正常调用方式；
- 性能优化；
- CI、构建、打包、checksum、发布流程修复；
- 文档修正；
- 内部重构且 CLI、配置、备份格式和协议行为保持兼容。

只有文档变化且用户没有要求“发布新版”时，可以不创建新 Release；一旦要正式发布，则按
PATCH 递增，不能复用旧 tag。

### MINOR：`1.0.x -> 1.1.0`

适用于向后兼容的新能力，例如：

- 新增子命令；
- 新增可选参数或输出能力；
- 新增平台支持；
- 新增备份/检查/诊断能力；
- 新增兼容模式，但旧用法仍然有效。

### MAJOR：`1.x.y -> 2.0.0`

适用于对用户或自动化脚本存在破坏性变化，例如：

- 删除或重命名已有命令/参数；
- 改变已有参数含义；
- 改变稳定输出格式导致脚本不兼容；
- 改变配置格式且不能无损直接读取旧配置；
- 改变备份格式且旧版备份无法继续恢复；
- 改变设备识别、写盘安全语义或协议结果，使既有调用产生不同含义；
- 放弃某个已正式支持的平台或架构。

如果同一版本同时包含多类变化，选择其中最高级别的版本变化。例如同时有 Bug 修复和新增
兼容功能，使用 MINOR；同时存在破坏性变化，则使用 MAJOR。

## 正式发布门禁

每次正式发布必须满足：

1. `main` 工作区 clean，`HEAD == origin/main`；
2. `Cargo.toml` 版本已经按本规范递增，`Cargo.lock` 同步；
3. `cargo fmt --all -- --check` 通过；
4. `cargo test --all-targets --locked` 通过；
5. `cargo clippy --all-targets --locked -- -D warnings` 通过；
6. 固定版本 Runner 的 macOS / Linux / Windows arm64 + x86_64 六架构 CI 全绿；
7. Linux / Windows arm64 + x86_64 虚拟磁盘 HIL-lite 门禁全绿；
8. tag 与 `Cargo.toml` 版本完全一致；
9. 三平台 arm64/x86_64 以及 macOS Universal 共七套 Release 产物全部生成；
10. 三个平台的 SHA-256 sidecar 独立校验通过；
11. macOS Universal 包确认同时包含 `arm64` 与 `x86_64`；
12. Linux 两个包分别确认是预期 ELF arm64/x86_64，Windows 两个包分别确认是预期 PE
    arm64/x86_64。

如果任一平台构建或验收失败，不创建残缺的正式 Release。

## Rust 与 Runner 基线

- Release Rust 工具链固定为 `rust-toolchain.toml` 中指定的版本；当前为 `1.98.1`。
- 正式 CI / Release 使用固定系统镜像和原生 CPU 架构 Runner，避免 `*-latest` 静默切换，
  也避免把交叉编译成功误当成目标平台原生验证成功。
- 另设 `latest` 兼容性工作流用于提前发现未来操作系统或 Rust stable 的兼容问题，但它不改变
  正式 Release 的构建基线。

## 发布供应链材料

每个正式 Release 除三平台二进制和 SHA-256 sidecar 外，还自动附带：

- CycloneDX 1.5 SBOM：`edpcli-vX.Y.Z-sbom.cdx.json`；
- 完整 `Cargo.lock`；
- `cargo metadata --locked` JSON；
- `rustc -Vv` 工具链记录；
- `release-manifest.json`：记录 tag、commit、固定 Runner、Rust 版本、每个发布资产的大小和
  SHA-256。

GitHub Actions 均锁到不可变 commit SHA，避免同名 action tag 被上游移动后悄然改变构建。

GitHub 官方 artifact attestation 对私有/内部仓库要求 GitHub Enterprise Cloud；当前仓库
不能把这一能力作为必过门禁，因此不配置一个注定失败的 attestation job。如果仓库未来迁移
到满足条件的套餐，应按 GitHub 官方 `actions/attest` 流程增加 `id-token: write` 与
`attestations: write` 后再启用。Apple Developer ID notarization 和 Windows Authenticode
同样需要外部签名证书；在证书存在之前，发布流程以 checksum + SBOM + manifest 保证可核验性。

## 无物理 Linux/Windows 机器时的验收口径

GitHub-hosted Runner 没有真实 EDP USB 硬件，因此不能声称完成真实 USB 总线硬件在环验证。
项目采用三层替代验证：

1. **原生平台 CI**：在真实 Linux/Windows 内核/API 环境运行完整测试与 CLI；
2. **虚拟磁盘 HIL-lite**：在临时 loop/VHD 上执行真实 raw block I/O、同步、卸载/锁卷、
   原子写入、读回和恢复；
3. **USB 身份契约测试**：使用可重复 fixture 覆盖 VID/PID、UAS/BOT、系统盘 fail-closed、
   selector 与枚举规则。

上述三层可以显著提高可信度，但仍必须在发布说明中诚实区分“虚拟磁盘验证”和“真实 USB
硬件验证”。如果未来具备实体 Linux/Windows 主机，应新增真实 USB HIL，而不是删除现有
虚拟磁盘测试。

当前 `Virtual Disk HIL` workflow 在 Linux/Windows 的 arm64 与 x86_64 Runner 上分别执行，
实际覆盖为：

- Linux：创建临时磁盘镜像 → loop 整盘 → MBR 分区 → ext4 → 挂载 marker → 产品
  `prepare_write` 执行 `umount2` → 对真实 `/dev/loopN` 做 LBA0-13 原子写/同步/读回 →
  bit-for-bit 恢复 → 重新挂载并验证 marker；
- Windows：创建临时 VHDX → MBR/NTFS/盘符 → 写 marker → 产品 `prepare_write` 执行
  volume extent 归属确认、`FSCTL_LOCK_VOLUME`、`FSCTL_DISMOUNT_VOLUME` → 对真实
  `\\.\PhysicalDriveN` 做 LBA0-13 原子写/同步/读回 → bit-for-bit 恢复 → detach/reattach
  VHDX 并验证 marker；
- CI 专用入口编译期默认关闭；Linux 只接受 `/dev/loopN`，Windows 只接受系统 API 返回
  `BusTypeVirtual` / `BusTypeFileBackedVirtual` 的磁盘，因此测试入口不能指向真实 USB 盘。

## 发布步骤

标准流程：

```text
功能分支
→ 测试/审计
→ PR CI 全绿
→ 合并 main
→ main CI 全绿
→ 根据本规范决定版本号并确认 Cargo.toml/Cargo.lock
→ 创建 vMAJOR.MINOR.PATCH tag
→ Release workflow 构建三平台
→ 下载 Release 资产做独立 checksum/架构验收
→ 完成发布
```

不得在 CI 尚未验证时先创建正式 Release，也不得手工上传一个平台成功、另一个平台缺失的
半成品正式版本。
