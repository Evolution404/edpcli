# edpcli 版本与发布规范

本文档是后续维护和 AI 继续开发时的版本号唯一决策基线。除非用户再次明确要求重置版本线，
不得自行跳号、回退、复用已经发布过的版本号或重新解释版本规则。

## 当前版本线

- 正式版本线从 **1.0.0** 开始。
- `Cargo.toml` 中的 `package.version` 是源代码版本的唯一事实源。
- 正式 tag 必须使用 `vMAJOR.MINOR.PATCH`，并且必须与 `Cargo.toml` 完全一致。
- GitHub Release workflow 会校验 tag 与 `Cargo.toml`；不一致时拒绝发布。

2026-09-18 经用户明确要求，项目版本线从此前的 `3.0.0` 重新起算为 `1.0.0`。这是一次
明确授权的版本线重置，不应被后续 AI 当作一般惯例重复执行。

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
4. `cargo test --all-targets` 通过；
5. `cargo clippy --all-targets -- -D warnings` 通过；
6. 固定版本 Runner 的 macOS / Linux / Windows CI 全绿；
7. 虚拟磁盘 HIL-lite 门禁全绿；
8. tag 与 `Cargo.toml` 版本完全一致；
9. 三平台 Release 产物全部生成；
10. 三个平台的 SHA-256 sidecar 独立校验通过；
11. macOS Universal 包确认同时包含 `arm64` 与 `x86_64`；
12. Linux 包确认是预期 ELF 架构，Windows 包确认是预期 PE 架构。

如果任一平台构建或验收失败，不创建残缺的正式 Release。

## Rust 与 Runner 基线

- Release Rust 工具链固定为 `rust-toolchain.toml` 中指定的版本；当前为 `1.98.1`。
- 正式 CI / Release 使用固定系统镜像，避免 `*-latest` 静默切换导致同一个版本不可复现。
- 另设 `latest` 兼容性工作流用于提前发现未来操作系统或 Rust stable 的兼容问题，但它不改变
  正式 Release 的构建基线。

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
