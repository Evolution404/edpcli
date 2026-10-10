# edpcli 全局架构审计（2026-10-10）

> 审计性质：基于当前源码、现有门禁及项目文档的只读架构检查，不是实体盘验收。基线：`feat/native-4kn-wal-staging-20261010`，HEAD `b57610ad`。保留其他 AI 的 worktree、编译及 TUI 进程，不执行实体盘写入。以下将**已证实的不一致**、**需验证的安全缺口**和**功能尚未交付**分开陈述。

## 0. 总结

正式 CLI/TUI 制盘已收敛到 `src/application/provision/native_flow.rs::prepare_native_provision_on_disk`，原生事务在 `native_commit.rs` 和 `TargetSession` 执行。25 对来源→目标组合的破坏性 WAL 虚拟设备测试有既有证据；新的默认保留逻辑和 1024/2048/4096B EDPB 只读取证测试已落地。**仍未实现完整的四规格等价用户体验与可恢复备份**。最大的架构风险不在单纯文件数量，而在旧入口和业务前置策略与统一 Application 不一致。

## 1. 优先级与证据

| ID | 等级 | 观察及性质 | 源码证据 | 影响／建议 |
|---|---|---|---|---|
| A01 | **P0 已证实** | **TUI 来源密码探测仅允许 512/4096B**，并且测试明确把 1024/2048B 列为拒绝对象；正式原生制盘的能力矩阵已有四规格。 | `src/tui/native_source_password_policy.rs:4-18`、`src/tui/controller/provision.rs:24-34`、`src/tui/provision/key_domains.rs:29-52`、`src/tui/dispatch.rs:282-296` | 1024/2048B 磁盘无法走正常的来源密码自动/手动验证路径，可能导致计划不必要地降级为 Unknown 或拒绝无损改密。改为统一 `native_sector_capability`+Application 验证；测试四规格正例及8192负例。|
| A02 | **P0 待风险验证** | **NativePreparedProvision 保存的 `before_pin` / `probe` / `device_id` 未传入正式原生提交函数**。提交阶段比较容量、逻辑块大小、LBA0–12 完整原生字节，`TargetSession` 也做重开身份和几何校验；但是否将规划时 USB 硬件身份与 LCE/来源分区首块绑定尚未建立显式契约。 | `src/application/provision/native_flow.rs:20-34,907-938,1139-1152`、`src/application/provision/native_commit.rs:49-108` | **不可据此断言已发生误写盘**。但应将规划时的设备身份与关键来源范围作为不可变 `SourceSnapshot`，由事务重开后在写锁内复核。引入盘号复用、同几何不同 VID/PID/序列号、LCE/启动块变化及 USB 拔插故障注入测试。|
| A03 | **P0 功能缺口** | **EDPB v4 明确 evidence-only**，不能恢复 EDP 协议/分区结构，更非用户数据备份；512B 的 v3 可恢复元数据仍是独立路径。四规格的“备份→同几何恢复”目标未交付。 | `src/edpb/write.rs:13-43,120-131`、`src/edpb/validate_native.rs:5-32`、`src/application/write/backup.rs:159-215`、`src/application/write/restore.rs:339-375` | 统一 EDPB 完整原生范围描述、身份/几何核对、来源完整性验证、可恢复写集合及 WAL。先实现与原生扇区相同的只读恢复计划，再经独立 OS 虚拟盘验收升级恢复合约；**不能直接把 v4 的 evidence-only 标记改为 restorable**。|
| A04 | **P1 已证实** | **另有 4Kn Mode0→Mode1 专用计划/写集业务链**：`provision plan --source-backup`、`native_preflight.rs`、`plan_verified_native_4kn_mode0_to_mode1`；TUI 保留对应旧异步任务和旧 review 状态（其发起入口需进一步确认是否还有生产消费者）。正式 `plan/image/write` 则已有统一 native_flow。 | `src/cli_args/provision.rs:683-718`、`src/cli/commands/provision.rs:373-416`、`src/application/provision/native_preflight.rs:9-260`、`src/application/provision/native_image.rs:714+`、`src/tui/provision/task.rs:157-183` | **统一场景只保留一条计划业务实现**。将 `--source-backup` 保留为通用“来源证据校验/规划输入”，直接使用 `NativeWritePlan`；原始离线样本生成器降级为测试 fixture 或独立工具，逐项迁移测试后删除专用逻辑和状态。|
| A05 | **P1 已证实** | **Native TUI 确认模型丢失加密算法字段**：来源请求含用户选择 `encryption_algorithm`，但 `PreparedProvision::Native` 不持有该选项，确认视图对于 Native 固定 `algorithm: None`。 | `src/tui/provision/validation.rs:455`、`src/application/provision/native_flow.rs:20-34`、`src/tui/provision/review.rs:638-658` | 对不同算法尤其是密钥封装与底层数据算法，确认页无法从真实不可变计划展示选择。把算法与密钥生命周期纳入统一确认投影，不能从编辑表单事后重新推断。|
| A06 | **P1 已证实** | **CLI/TUI 的格式化策略仍在接入层分散**：CLI 默认 `preserve_unformatted=false`，规划器按不兼容区自动格式化；TUI 固定 `true` 并在表单前置预检中选中必需格式化。最终都用 `TargetProvisionPlan`，但策略输入及呈现不是同一来源。 | `src/cli_args/provision.rs:102,514-527`、`src/application/provision/native_flow.rs:228-278`、`src/tui/provision/validation.rs:489-506` | 统一 Application `ProvisionIntent` 和 `RequiredRebuildDecision`；CLI/TUI 仅选择交互方式，所有来源丢弃/保留、授权确认均以同一决策对象生成。构建 25 组合 × 四规格 × 密码/FS/几何变量的同构测试。|
| A07 | **P1 可靠性缺口** | 真实 OS 原生虚拟盘此前虽完成四规格 25 对破坏性转换，但无损保留、部分保留、仅改密、LCE/保密区全区原始 SHA256、实体 USB 拔插及故障回滚未形成等价证据。4Kn 同模式 WAL 成功的 SHA256 仅覆盖可由 MBR 枚举的启动区。 | `audit/project/MULTI_LOGICAL_SECTOR_FULL_ACCEPTANCE_PLAN_2026-10-10.md` S1-B/S5；`audit/project/MULTISECTOR_VIRTUAL_OS_E2E_ACCEPTANCE_2026-10-10.md` | 不是可断言存在 bug；补真实 OS 设备四规格只读前后全范围哈希，特别覆盖密文及 LCE、重挂与密码验证、失败注入与回滚；完成前不宣布无损验收。|
| A08 | **P1 已证实** | **旧 512/4Kn 白名单继续分散**：`verified_native_source_replay` 仅接受 512/4096B，另有仅供 LCE 诊断的 512/4096B 限制。诊断工具可能有刻意范围限制，不应盲目扩展；但生产功能不宜各自定义能力矩阵。 | `src/application/evidence.rs:828-844`、`src/provision/native_lce_audit.rs:25-35`、`src/domain/hardware.rs:18-45` | 分类“生产能力 / 仅支持实验样本 / 仅旧版兼容”并由统一能力 Registry 生成允许矩阵；能验证的场景使用四规格通用几何。|
| A09 | **P2 已证实** | 原生 WAL 被包装成 `MetadataBackupReport`，同时 `edp_protocol_saved=true` 且原生结果 `formats: vec![]`；UI 已按 `.wal` 做区分，但模型语义仍把**事务回滚日志**放到**用户元数据备份**槽位。 | `src/application/provision.rs:942-1012`、`src/tui/provision/result_presentation.rs:18-30` | 拆分 `TransactionJournalReport`、`MetadataBackupReport`、`PartitionFormatReport` 三类，UI 标题、恢复能力、后续操作不得混淆。|
| A10 | **P2 已证实** | 旧 `prepare_provision_on_disk` / `PreparedProvision::{Official,Plain}` 仍在公有 Application API；TUI 统一确认投影实际上拒绝它们，却保留了多条独立领域/写入模块和测试。 | `src/application/provision.rs:528+,694-708`、`src/application/provision/prepare.rs`、`src/tui/provision/review.rs:638-640` | 先跑消费者清单，区分真实生产调用和测试旧 API，逐步收敛为唯一 `PreparedProvision::Native` 正式路径；保留历史协议证据和独立离线 fixture，不做一次性删文件。|
| A11 | **P2 已证实** | 架构权威文档和 CLI 镜像输出有旧叙述：“所有模式转换是破坏性重建”“不保留 FileKey”，与 `9fc54cc1` 后已实施默认保留策略冲突。 | `docs/architecture/ARCHITECTURE.md:59`、`docs/architecture/UNIFIED_NATIVE_DISK_COMMIT_2026-10-10.md:23`、`src/cli/commands/provision.rs:567` | 将文档区分“历史验证时的破坏性矩阵”和“当前按来源感知保留默认行为”，CLI/帮助从真实 `SourceImpact` 生成，避免继续误导用户。|
| A12 | **P2 待校验** | `--include-virtual` 使用进程级 `AtomicBool`，不是按每次 DeviceAccess/Session 注入的显式发现策略；当多个测试/任务共进程时可能相互影响，尽管目标写入另行验证硬件身份。 | `src/platform/mod.rs:226-242`、`src/cli.rs:104-109`、`src/platform/system.rs:36-40` | 长期移至 `DeviceDiscoveryPolicy` 或 `TargetSession` 上下文；增加同进程开/关、异步扫描竞态测试。不能把全局开关当作写授权。|
| A13 | **P2 待 OEM 行为确认** | Native 重建对目标分区单独 `getrandom` 生成 FileKey，正式 CLI 当前硬编码 SMS4，TUI 有算法选择；官方多记录是否复用同一 FileKey、密码域/分区实际加密算法是否完全对应尚需真实原厂样本交叉验证。 | `src/application/provision/native_flow.rs:548-564`、`src/cli/commands/provision.rs:13-18`、`docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md` | 不将证据不足的“与官方不一致”当作已证实 bug。建立 per-record FileKey 生命周期和真盘原厂多分区样本金标，确认“新建、保留、仅改密、重建”的共同规则。|
| A14 | **P3 治理问题** | TUI 和 Provision 模块大而多层，存在历史回调状态与通用 TUI 渲染态重叠；冗余审计有 99 个单跳转发函数、76 个仅测试消费 API、11 个未引用 API，但没有登记出确定可删除的问题。 | `src/tui/provision/`（57 文件）、`src/application/provision/`（19 文件）；`target/redundancy-audit/report.md` | 先按业务调用链进行“可删/需保留/证据档案”分类，再精简测试专用符号和转发链。坚决不按静态候选数量直接清理。|

## 2. 已证实、暂不应推翻的基础

1. 目前正式 CLI `plan/image/write` 和 TUI `request_provision_plan` 调用同一个 `prepare_native_provision_on_disk`；正式写入使用 `PreparedProvision::Native`、`native_commit`、`TargetSession`、原生 WAL，不应再新增旁路。
2. `project_native_impact` 以来源分区 ID、几何、角色、实际写集计算保留/丢弃，存在覆盖保留区即拒绝的检查：`src/application/provision/native_flow.rs:1026-1127`。这层是需要维护的统一真相源。
3. EDPB v4 验证器按 1024/2048/4096B 原生几何逐块核验 LBA0–12、LCE 及证据原始 extent；`src/application/evidence.rs:742-821`。验证来源一致不授予恢复写权限。
4. `TargetSession` 在取得写租约后重开设备并检查几何和协议块，存在有价值的基础身份防护。A02 是**规划时证据绑定链完整性**的待验证项，不是声称完全没有身份防护。

## 3. 门禁／本轮执行情况

- `scripts/test-fast.sh`：8 套测试、10 个编译产物、失败 0，耗时 **56.76s**；包含表格双边横滚门禁及 PTY replay 工程门禁。
- `uv run --locked python scripts/audit-redundancy.py --check`：扫描 **851 文件**，`unregistered_rust=0`、`missing_module=0`、`broken_document_link=0`、`retired_symbol=0`，确认问题 **0**，人工候选 **216**（99 forwarding、76 test_only_api、11 unreferenced_api 等）。不是 216 个确定缺陷。
- Git：本轮审计开始时 `b57610ad`，工作区干净；没有执行实体盘写入、没有检查 GitHub CI、没有修改正在运行的其他 AI 工作树。完整 OS 虚拟盘和物理硬件验收不计入本轮。

## 4. 执行顺序（最小可验证改造）

1. **P0 安全与正确性**：A01 四规格 TUI 密码验证和 A02 来源硬件+协议+LCE/分区首块冻结身份，补无损转换下的错盘/拔插/篡改拒绝测试。A05 确认页算法显示可同批修复。
2. **P1 架构唯一性**：A04 迁移/退役 Mode1 专用 `--source-backup` 预检与 TUI 无生产调用旧状态，A06 统一 CLI/TUI 格式化意图，A08 统一扇区能力矩阵。
3. **P1 恢复功能**：A03 完成 EDPB v4 同几何恢复只读计划和正式 WAL 事务验收；禁止在没有完整独立恢复证据前修改 `evidence-only` 权限。A07 的全范围哈希、加密、重挂和事务故障测试与此联动。
4. **P2 收口**：A09 拆分 WAL 与备份/格式化结果类型；A10 清理公有但不可使用的旧 API；A11 同步权威文档和 CLI 文案；A12 消除进程级发现开关；A13 核对真实 OEM FileKey 金标；最后对 A14 216 候选逐项裁决。
5. **最终门禁**：每个阶段 `fmt` + 受影响单测 + `scripts/test-fast.sh`；重大改造执行项目 full runner、四规格真实 OS 虚拟设备端到端验收；实体 USB 另获明确授权；**GitHub CI 仅在合并 main 前再处理**。

**决策原则**：先补“真实行为错误或数据丢失风险”，再移除双路径，最后做文件级清理；严格分清已证实错误、潜在风险和未交付功能。
