# edpcli 功能一致性、测试性能与架构审计报告

> 日期：2026-09-25
> 基线：`main@559a79751f1bf7a53109332bd1c02a726c58c94f`
> 范围：CLI/TUI 功能一致性、测试执行性能、架构与技术债、历史兼容/未使用代码、后续优化路线。
> 本报告只给出审计结论和实施顺序，不在本次审计中直接重构生产代码。

## 1. 执行摘要

当前项目的协议层和核心安全写盘链已经具备较高完成度，但产品入口和工程组织已经出现明显的“能力先增长、架构后追赶”现象。本轮最需要优先处理的不是协议语义，而是前端能力模型、命令事实源、测试组织和大文件拆分。

本次审计确认四个核心结论：

1. **CLI/TUI 功能并不完全一致。** 最明确的缺口是 Plain：TUI 已把 Plain 作为与 mode0～mode3 并列的第五种制盘目标，并且走正式 `prepare_plain_provision()/commit_plain_provision()` 安全链；CLI 的 `provision` parser 和 `ProvisionAction` 仍只接受 `--mode 0|1|2|3`，Plain 不是 CLI 一等能力。
2. **当前测试“慢”主要不是测试逻辑慢，而是 70 个 integration-test crate 带来的 Cargo 编译/链接/调度成本。** 已编译后的多个测试二进制实际只需几十毫秒到数秒；但单独 `cargo test --test X` 的暖缓存调用仍约 12 秒起步。继续要求 AI 在 120 秒调用内硬跑 `cargo check --all-targets + cargo test --all-targets` 是结构性不可靠的。
3. **架构存在多个单点“大模块”和重复事实源。** `tui/state.rs` 约 5.5k 行、`tui/render.rs` 约 3k 行、`application/provision.rs` 约 2.1k 行、`diskio.rs` 约 1.7k 行。CLI parser/help/completion 也分别手工维护，已经实际产生删除 Offline Convert 后 completion 仍暴露 `convert` 的漂移缺陷。
4. **“删除历史兼容”必须分两类处理。** 可删除的是产品层旧命令、旧 `.bin` 迁移工具、无生产调用的旧 helper 等；不能因为名字带 `legacy` 就删除协议兼容解析。LCE、`legacy`-v0064、历史 MBR underlay、旧 wrapped-key 等属于真实设备协议事实与当前解析/验证能力。

建议后续重构遵循：

> **P0：测试基础设施 → P0：CLI/TUI 能力对齐 → P0：命令单一事实源 → P1：应用层/前端拆分 → P1：删除明确死代码和一次性迁移兼容 → P2：进一步性能与工程约束。**

---

## 2. 审计证据

### 2.1 当前代码规模

| 文件 | 约行数 | 审计判断 |
|---|---:|---|
| `src/tui/state.rs` | 5,551 | God `state`，必须拆分 |
| `src/tui/render.rs` | 3,052 | God renderer，按 workspace 拆分 |
| `src/inspect.rs` | 2,287 | `canonical` parser 上仍承载大量 view 语义 |
| `src/application/provision.rs` | 2,121 | official/plain/commit/export 多职责 |
| `src/tui/mod.rs` | 1,685 | event loop/dispatch/elevation/resume 集中 |
| `src/diskio.rs` | 1,667 | block I/O、transaction、backup catalog/config 混杂 |
| `src/tui/task.rs` | 1,402 | generation/single-flight worker 编排重复 |
| `src/cli.rs` | 1,396 | CLI orchestration 偏重 |
| `src/cli_args.rs` | 1,223 | 手写 parser/help 事实源之一 |
| `src/provision/reprovision.rs` | 1,166 | mode/prefill/转换语义集中 |
| `src/application/inspect.rs` | 1,058 | 新统一 `Inspect` backend |

`src/` Rust 代码总量约 43k 行；顶层 integration tests 约 22k 行。

### 2.2 测试目标数量

`tests/*.rs` 当前约 **70 个独立 integration-test target**。每个文件都是独立 Rust test crate，需要 Cargo/rustc 单独处理并生成/运行测试二进制。

---

# 第一部分：CLI / TUI 功能一致性

## 3. 一致性原则

这里的“一致”不是界面完全相同，而是：

- 同一业务能力必须走同一 application/domain 实现；
- CLI/TUI 应能访问相同的核心业务目标；
- TUI 专属导航、Tab、Refresh、Quit 不要求映射到 CLI；
- CLI 专属 completion/version/help 不要求映射到 TUI；
- 若能力只适合一个前端，必须显式定义为前端专属呈现，不能因为底层模型分叉而缺失。

建议最终建立 capability registry 和 parity test，而不是继续靠人工记忆。

## 4. 制盘：当前最大的不一致

### 4.1 TUI 已支持五种目标

`src/tui/state.rs` 的 `ProvisionKind`：

- Mode0
- Mode1
- Mode2
- Mode3
- Plain

`ProvisionKind::ALL` 包含五种目标，Plain 映射到 `ProvisionTarget::Plain`。

TUI Plain 正式路径：

```text
ProvisionPlanInput::Plain
  -> prepare_plain_provision()
  -> ProvisionPrepared::Plain
  -> commit_plain_provision()
```

这已经是正式产品能力。

### 4.2 CLI 仍只支持 Official mode0～3

`src/cli_args.rs` 的 `ProvisionAction` 只有：

- `Plan(ProvisionNewOpts)`
- `Image { opts: ProvisionNewOpts, out }`
- `Write { opts: ProvisionNewOpts, yes }`

parser 明确要求：

```text
provision 新盘操作必须指定 --mode 0|1|2|3
```

`src/cli.rs` 又进一步假设：

```text
ProvisionTarget::from_mode_number(opts.mode)
expect("CLI parser validates provision mode to 0..3")
```

因此 Plain 不是 CLI 的一等 `ProvisionTarget`。

### 4.3 正确修法

不要增加 `--mode 4`。Plain 明确不是 mode4。

推荐把 CLI 目标升级为 typed target，例如：

```text
edpcli provision plan  --disk 4 --target mode0 ...
edpcli provision plan  --disk 4 --target mode1 ...
edpcli provision plan  --disk 4 --target mode2 ...
edpcli provision plan  --disk 4 --target mode3 ...
edpcli provision plan  --disk 4 --target plain ...
```

如果考虑短期兼容，也可继续接受 `--mode 0|1|2|3`，同时增加显式 `--plain`，但 parser 最终都必须映射到同一个领域 target，而不是 CLI 再维护第二套目标枚举。

CLI Plain 至少要覆盖：

- plan
- write
- 默认 P1 从 LBA2048 到磁盘末尾
- 1～4 个 MBR primary partition
- start LBA
- sector count / MiB / GiB
- fill 语义
- `filesystem` / label
- gap
- `--yes`
- 与 TUI 完全相同的 system-disk guard、USB guard、backup、reopen、transaction、readback、rollback

**禁止 CLI 自己实现一套 Plain `writer`。**

## 5. Provision image/export

Official mode0～3 在 CLI/TUI 均有制盘能力，但 export 目前是 Official-only：

- CLI 有 `provision image`；
- TUI Export 只接受 `ProvisionPrepared::New`；
- `ProvisionPrepared::Plain(_) => return None`。

需要产品层明确选择：

### A. 保持 Official-only

若 Plain 定义为“恢复普通盘，不提供离线镜像”，则在文档和 capability registry 明确标记，不算 parity 缺口。

### B. Plain 也支持 image/export（推荐）

Plain 已有确定性的 `PlainProvisionWritePlan`，可新增统一 application exporter，然后 CLI/TUI 同时开放。

## 6. Backup 功能矩阵

| 能力 | CLI | TUI | 结论 |
|---|---|---|---|
| create `metadata` | 有 | 有 | 一致 |
| create `deep` | `--deep` | 独立入口 | 语义一致 |
| list | 有 | Backups workspace | 一致 |
| `verify` | 有 | 有 | 一致 |
| `restore` | 有 | 有 | 一致 |
| delete single/multi | 有 | 有 | 一致 |
| prune | 有 | 有 | 一致 |

Backup 目前重点不是补功能，而是继续保证两端调用同一 application service。

## 7. `Inspect` 功能矩阵

### 已统一的核心

CLI/TUI 已共享 `application::inspect`：

- SectorReader
- `decode_sector()`
- `sector_meta_text()`
- topology
- Protocol/LCE/Partition decoder

这是正确方向。

### 合理的前端差异

TUI 有 full-disk tree、lazy topology、`Hex`/ASCII、byte cursor、Field ↔ `Hex`、jump/search、Raw/Decode/Mixed，这些属于交互式 UI，不要求 CLI 原样模拟。

### 仍有的语义差异

CLI Advanced `Inspect` 支持 `export_dir`；TUI 构造请求时固定 `export_dir: None`。

如果导出选定扇区/解析结果被认定为核心业务能力，应给 TUI 增加 Export；否则明确标成 CLI automation surface。

## 8. 命令补全已经发生事实源漂移

Offline Convert 产品入口已删除，parser/TUI 也明确拒绝 `convert`，但 `src/completion.rs` 仍生成：

- zsh：`plan image write convert`
- bash：`plan image write convert`
- fish：`plan image write convert`
- 以及 `convert` 对应参数分支

这是已存在的用户可见 `bug`。

根因是 parser、usage/help、completion、TUI palette、docs/tests 分散手工维护。

### 必须修复

建立单一 command schema，例如：

```rust
struct CommandSpec {
    name: &'static str,
    aliases: &'static [&'static str],
    options: &'static [OptionSpec],
}
```

由它至少驱动：

- help
- shell completion
- surface contract test

parser 可以继续手写，但 subcommand/flag catalog 不应重复。

也可以评估引入 clap + completion generator；无论是否引入依赖，“单一事实源”是硬要求。

---

# 第二部分：测试性能与 120 秒限制

## 9. 测试为什么慢

### 9.1 测试 `body` 并不普遍慢

本轮直接运行已经编译好的测试二进制：

| target | 直接运行 |
|---|---:|
| `inspect` | ~0.036s |
| `backup` | ~0.254s |
| `atomic_write` | ~1.063s |
| `provision_protocol_audit` | ~3.133s |
| `tui_state` | ~5.672s |

但逐个调用：

```text
cargo test --test <target> --quiet
```

暖缓存仍观察到：

| target | Cargo 调用 |
|---|---:|
| atomic_write（首个） | ~27.7s |
| backup | ~12.2s |
| backup_catalog | ~12.2s |
| backup_deep | ~12.2s |

主要成本是 integration crate 数量、rustc/check/link 调度和 debug/link I/O，而不是断言本身。

### 9.2 `cargo check --all-targets` 已占大半预算

本轮实测：

```text
cargo check --all-targets
≈ 64s
0 warnings
```

因此在一个 120 秒工具调用内串行执行 check + full test，本来就不可靠。

### 9.3 当前 CI wrapper 只改善日志

`scripts/ci/run-cargo-test-ci.py` 仍只是运行：

```text
cargo test --all-targets --locked
```

没有 shard、nextest、per-target timeout 或 fast/full 分层。

## 10. 测试重构方案

### 10.1 将约 70 个 integration target 合并为 8～10 个 suite

推荐：

```text
tests/
  cli_suite.rs
  cli/...

  backup_suite.rs
  backup/...

  inspect_suite.rs
  inspect/...

  provision_suite.rs
  provision/...

  protocol_suite.rs
  protocol/...

  tui_suite.rs
  tui/...

  platform_suite.rs
  platform/...

  hil/
```

减少 test crate / 链接单元，不减少测试用例。

### 10.2 HIL 与普通门禁彻底分离

分类：

- unit/`core`
- integration/`core`
- protocol audit
- virtual HIL
- real USB HIL

默认 fast/full 非 HIL gate 不应编译或运行真实 HIL。

`examples/hil_provision_front.rs` 等 HIL helper 建议移动到 `tools/hil/` 或显式 feature/bin target，避免每次 `--all-targets` 都编译。

### 10.3 建立官方 fast/full 命令

新增：

```text
scripts/test-fast.sh
scripts/test-full.py
```

#### fast

目标：本机稳定约 30～45 秒。

包含：

- `cargo fmt --all -- --check`
- `git diff --check`
- 核心 suite
- 受影响领域 suite

不要每次都硬跑 `cargo check --all-targets`。

#### full

用于：

- 合并前
- 发布前
- 大规模重构后
- 用户明确要求完整验收

完整运行全部非 HIL tests。

### 10.4 AI/120 秒环境硬规则

在 `AGENTS.md` 写明：

> WebCodex/AI 不得在一个 120 秒同步 shell 调用里串行执行 `cargo check --all-targets` + `cargo test --all-targets`。Full gate 必须走仓库提供的 full runner；在支持 durable/detached job 的环境中 timeout ≥600s 并 observe 同一个 Job 到最终 exit code。

这样后续 AI 不再重复本轮超时模式。

### 10.5 推荐 nextest

优先评估：

```text
cargo nextest run
```

配 `.config/nextest.toml`：

- 并行 test binaries
- slow timeout
- `profile`：fast/full/ci
- slow test reporting

CI 安装预编译 nextest。

注意 doctest 仍需单独执行。

### 10.6 不增加 nextest 的备选

实现 `scripts/test-full.py`：

1. 一次 `cargo test --all-targets --no-run --message-format=json`；
2. 从 Cargo `compiler-artifact` JSON 获取本次真正的 test executables；
3. 排除 HIL/ignored-only target；
4. 以 2～4 并发运行安全 test binaries；
5. 每个 binary 独立 timeout；
6. 汇总 pass/fail/timeout/duration。

不要正式采用“扫描 target/debug/deps 猜测试二进制”的方式。

## 11. 其它测试优化

### P1

测量：

```toml
[profile.test]
debug = 0
```

或 `debug = 1`，重点看 macOS link/debug-info I/O。

### P1

CI 引入 sccache。

### P1

变更范围门禁：

| 变更 | 必跑 |
|---|---|
| docs-only | documentation contract |
| CLI/completion | cli suite |
| TUI | tui suite + related application |
| `Inspect` | `inspect` suite + protocol subset |
| Provision | provision + protocol + write safety |
| Protocol | protocol + provision + `inspect` + full |
| release | full + virtual HIL |

### P2

持续记录 suite duration，并建立性能回归阈值。

---

# 第三部分：架构审计与重构

## 12. 目标分层

当前文档方向正确：

```text
CLI / TUI
  -> application/service
  -> domain/protocol/backup
  -> platform + disk I/O
```

建议进一步明确：

```text
presentation
  cli/
  tui/
      ↓
application
  device/
  backup/
  inspect/
  provision/
      ↓
domain
  protocol/
  provision/
  backup/
      ↓
infrastructure
  disk/
  platform/
  filesystem/
```

前端只负责 input/`state`/navigation/render，不负责业务策略、设备身份判断、事务或协议解释。

## 13. TUI God modules

建议拆为：

```text
src/tui/
  app.rs
  navigation.rs
  task.rs
  theme.rs

  devices/{state,render}.rs
  backups/{state,render}.rs
  inspect/{state,render}.rs
  provision/{state,render}.rs
```

`AppState` 只组合子 `state`。

门禁：

- workspace 不直接访问 platform/diskio；
- 异步 I/O 通过 task/application；
- render 不改变业务状态。

## 14. TaskHub 模式重复

当前为多种任务手工维护 generation gate、single-flight gate、pending latest、worker/result。

建议抽象：

```rust
LatestTaskSlot<Request> {
    generation,
    single_flight,
    pending_latest,
}
```

或 typed `TaskSlot<Req, Res>`，统一 start/latest-wins/`stale`-result discard。

关键写盘 `critical_worker` 应继续独立，不和普通只读任务混用。

## 15. Provision application 拆分

建议：

```text
application/provision/
  mod.rs
  request.rs
  prepare_official.rs
  prepare_plain.rs
  commit.rs
  export.rs
  report.rs
```

可进一步统一：

```rust
enum PreparedProvision {
    Official(...),
    Plain(...),
}
```

提供统一 `prepare()/commit()/export_if_supported()`。

## 16. `diskio.rs` 拆分

当前混杂 raw block device、transaction、backup config/naming/catalog/create、旧 helper。

建议：

```text
src/disk/
  device.rs
  transaction.rs

src/backup/
  catalog.rs
  naming.rs
  config.rs
  create.rs
```

过渡期可保留 re-export，完成迁移后删除 `diskio` 聚合文件。

## 17. `Inspect` / metainfo / provision `validator` 依赖漂移

新 `Inspect` backend 已统一到 `application::inspect`，但：

- `metainfo.rs`
- `provision/validate.rs`

仍直接调用 `inspect::analyze_sector()`。

这使 `inspect.rs` 仍是跨业务 `semantic` dependency。

目标应为：

```text
protocol typed parser / semantic facts
  ├─ metainfo
  ├─ provision validator
  └─ application::inspect field adapter
```

而不是：

```text
protocol -> inspect presentation -> metainfo/provision validator
```

把 ownership、partition、pass-info、onlyid、device identity、disk mode 等跨业务事实放到 typed protocol `semantic` layer；`inspect.rs` 仅做 SectorField/human-readable rendering `adapter`。

---

# 第四部分：历史兼容与未使用功能清理

## 18. 高置信可删除候选

本轮全仓库引用审计中，下列符号无生产调用或仅自身/测试调用：

### 18.1 `diskio::wildcard_match()`

只有自身 unit test。删除函数和测试。

### 18.2 `diskio::read_lba_file()`

旧 snapshot-directory helper，兼容 `LBA7.bin/LBA07.bin` 且缺失自动补零，只有自身测试使用。直接删除。

### 18.3 `diskio::backup_label_id()`

只有 `tests/backup.rs` 直接调用。正式 catalog 已从 EDPB/raw protocol 取得 onlyid。删除 convenience API，并让测试验证正式 catalog。

### 18.4 `diskio::backup_is_nopwd()`

只有 tests 调用；生产使用 `image_is_nopwd()` / catalog。删除路径 wrapper。

### 18.5 `application::provision::prepare_new_provision()`

全仓库搜索只找到定义，无 `caller`，已被 `prepare_target_provision()` 取代。高可信死代码。

### 18.6 `force_change_password_from_sectors()`

只有定义/re-export，无 `caller`；已有完整 `pass_info_policy_from_sectors()`。删除 wrapper。

### 18.7 completion 内全部 `convert`

明确缺陷，立即删除，并新增 surface guard。

## 19. 旧 `.bin/.sha256` 迁移兼容

正式运行时已经不读取旧 `.bin` 作为 backup。剩余兼容主要是：

- `examples/migrate_legacy_backups.rs`
- `edpb::write_legacy_migrated_backup()`
- `diskio::sha256_sidecar_path()`
- `diskio::read_backup_sha256()`
- `CaptureLevel::LegacyMigrated`

其中 `read_backup_sha256()` 只服务 migration example。

### 清理步骤

1. 先扫描用户实际 EDPB 是否还有 `capture_level=legacy_migrated`；
2. 若历史迁移已经结束，删除迁移 example、`writer` 和 sidecar helpers；
3. `LegacyMigrated` 是序列化 schema 值，若现存重要 EDPB 仍使用它，保留 read-only decode，但删除生成入口；若用户明确放弃这些文件，再删除 enum variant。

不要把 LegacyMigrated 静默解释成 `Metadata`/`Deep`，因为缺失 `artifact` 是真实语义。

## 20. 不能删除的“`legacy`”

以下是协议事实，不是产品兼容包袱：

- LCE = LBA7 `legacy` compatibility `extent`
- `legacy`-v0064
- `legacy` MBR snapshot/underlay
- `legacy` wrapped key
- historical protocol `profile` axes
- 对真实旧盘/真实免密盘的 parser

它们仍用于真实设备识别、协议闭环、`Inspect`、backup `metadata`、reprovision、`validator`。

原则：

> 删除旧产品入口和旧文件格式兼容，不删除真实介质上仍可能存在的协议格式解析。

## 21. 其它候选

以下需要实施阶段再次全仓检索后决定：

- `generate_image()`：正式 Official 写路径使用 `generate_official_image()`；
- `build_official_exfat_partitions()`：当前主要见于 tests；
- 测试夹具专用 public API 可缩为 `pub(crate)` 或 test helper。

逐个删除，小步提交，不做“大扫荡”。

---

# 第五部分：文档/事实源漂移

## 22. `ARCHITECTURE.md` 已过时

当前仍写“不要把 CLI/TUI provision 当作已实现产品能力”，已经不符合事实：

- Official CLI provision 已存在；
- TUI Provision 已存在；
- TUI Plain 已存在；
- Phase 8 已有真实 mode0→Plain PASS。

重构时必须同步更新。

## 23. 产品描述仍写“免密转换”

`Cargo.toml` 和 `src/lib.rs` 仍有“备份、恢复与免密转换”，但 Offline Convert 产品能力已删除。

建议改为：

> 识别、检查、备份、恢复与制盘

## 24. `scan_backup_names()` 注释漂移

实现只扫描 `.edpb`，但注释仍写“普通 `.bin` 文件”。应修正。

---

# 第六部分：进一步优化方案

## 25. Capability Registry

建立纯领域/应用层 capability：

```rust
enum Capability {
    DeviceList,
    DeviceInfo,
    BackupCreate,
    BackupDeep,
    BackupVerify,
    BackupRestore,
    BackupDelete,
    BackupPrune,
    Inspect,
    ProvisionOfficial,
    ProvisionPlain,
    ProvisionImage,
}
```

测试声明哪些 `required_both`、哪些 frontend-only，从根本上防止 CLI/TUI 漂移。

## 26. 统一 Provision Input

最终只保留 application request：

```rust
enum ProvisionRequest {
    Official(OfficialProvisionRequest),
    Plain(PlainProvisionRequest),
}
```

CLI/TUI 都只构造 request，application 统一 prepare/review/commit/export。

## 27. Raw-device TargetSession

抽象：

```text
TargetSession<ReadOnly>
TargetSession<PreparedWrite>
TargetSession<WriteLocked>
```

显式状态转换：

```text
open_readonly()
prepare_write()
reopen_and_verify()
commit()
```

统一 target selector、USB/system guard、probe、capacity、`metadata` snapshot、reopen identity，减少多条写链重复与权限状态隐式性。

## 28. EvidenceSource

统一 Disk / EDPB：

```rust
enum EvidenceSource {
    Disk(...),
    Backup(...),
}
```

共享 total sectors、protocol image、`artifact` lookup、sector `reader`、identity `metadata`。

## 29. Typed Report/Event

application 返回结构化 `ProvisionReport/BackupReport/InspectReport`，CLI/TUI 仅渲染。避免业务层以大段面向前端的字符串作为主要 contract。

## 30. 架构依赖门禁

新增静态测试/脚本禁止：

- provision/protocol -> tui/cli
- application domain logic -> ratatui/crossterm
- provision `validator` -> `inspect` presentation types

允许：

- cli/tui -> application
- application -> domain + infrastructure
- domain -> protocol primitives

## 31. 文件规模软门禁

建议：

- >2000 行：review warning
- >3000 行：必须有拆分说明

目标是防止新的 God module 继续增长，不是机械追求小文件。

---

# 第七部分：实施顺序

## Phase R0：先修测试基础设施

**先做这一阶段，否则后面所有重构都会继续被 120 秒限制拖累。**

1. 合并约 70 个 integration target 为 8～10 个 suite；
2. 建 `test-fast` / `test-full`；
3. full 使用 nextest 或 Cargo JSON + compiled-binary runner；
4. AGENTS.md 写明 120 秒执行规则；
5. CI 使用同一 full runner；
6. 输出 per-suite duration。

完成标准：

- fast gate 本机目标 <45s；
- full gate 可用 durable/detached Job 稳定拿最终 exit code；
- 后续 AI 不再直接用 120s shell 硬跑 `cargo test --all-targets`。

### R0 实施状态（2026-09-25）

**COMPLETE。**

- `Cargo` 已关闭自动集成测试目标发现：70 个顶层测试源收敛为 **8 个正式非 HIL 测试组 + 2 个显式功能开关控制的虚拟 HIL 目标**；测试用例本身未删减。
- `scripts/test-fast.sh` 已成为日常门禁：`fmt --check` + `git diff --check` + 核心测试组 + 按改动路径选择的领域测试组。当前 Mac 实测 **21.7s**，满足 `<45s` 目标。
- `scripts/test-full.py` 已成为正式完整非 HIL 门禁：一次 `cargo test --no-run --message-format=json` 获取真实测试可执行文件，随后以受控并发直接运行，每个测试程序具有独立超时，并输出各测试组耗时；文档测试单独执行。
- 完整门禁首次验证在 30 秒同步等待后被提升为持久任务，并通过观察**同一个任务**取得最终退出码；修正旧 CI 契约测试后，当前暖缓存完整门禁实测 **12.0s，0 个失败**。
- CI 已直接调用 `python scripts/test-full.py --profile full`；旧 `scripts/ci/run-cargo-test-ci.py` 暂保留为薄兼容入口，但不再自行执行 `cargo test --all-targets`。
- `AGENTS.md` 已写入 120 秒硬规则：AI 不得再在单个同步命令调用中串行硬跑 `cargo check --all-targets + cargo test --all-targets`，完整门禁必须使用仓库测试运行器与不少于 600 秒的持久任务观察模式。
- 虚拟 HIL、真实 HIL 与日常/完整门禁保持分离；本阶段未改变 LBA0～12/LCE 协议语义，也未降低任何写盘安全门槛。

## Phase R1：CLI/TUI 能力对齐

1. 抽统一 `ProvisionRequest::{Official,Plain}`；
2. CLI 增加 Plain plan/write；
3. 明确并实现 Plain image/export 策略；
4. 两端共用 application prepare/commit；
5. 建 capability parity test。

### R1 实施状态（2026-09-25）

**COMPLETE。**

- application 已建立统一 `ProvisionRequest::{Official,Plain}`、`PreparedProvision::{Official,Plain}`、`prepare_provision`、`commit_provision` 和 `export_provision_image`；CLI/TUI 的实际制盘执行不再各自直调 Official/Plain 写入实现。
- CLI 正式支持 `--target mode0|mode1|mode2|mode3|plain`；原 `--mode 0|1|2|3` 仅保留为四种官方模式的兼容输入。`--mode 4` 和 `--target mode4` 均明确拒绝，Plain 从未进入官方模式编号域。
- CLI Plain 已覆盖 `plan/write/image`。默认 P1 从 LBA2048 占满盘尾；可重复 `--partition START:SIZE:FS[:LABEL]` 建 1～4 个 MBR 主分区，支持扇区数、MiB、GiB、`fill` 与显式空闲区。
- Plain 稀疏镜像已由 application 统一导出，并保留目标盘 LBA3；TUI Review 阶段不再禁止 Plain 导出。
- TUI plan/write/export 已收敛到与 CLI 相同的 application prepare/commit/export 入口；TUI 表单层继续保留自身编辑状态，但不再维护第二套制盘执行路径。
- 新增 capability parity 门禁，检查 CLI/TUI 必须共享统一入口、不得回退到 `prepare_target_provision/prepare_plain_provision/commit_new_provision/commit_plain_provision` 直调，并锁定 Plain 非 mode4。
- R1 定向验证共 **371/371** 通过；R1 fast 门禁耗时 **37.53s**、0 个失败，仍满足 R0 的 `<45s` 目标。
- 本阶段未改变 LBA0～12/LCE 已闭环协议语义，也未降低 USB/系统盘保护、目标身份与容量复核、reopen、事务写入、逐扇区读回和 rollback 安全链。

## Phase R2：命令单一事实源

1. 建 CommandSpec；
2. help/completion 从统一 schema 生成；
3. 删除 completion `convert`；
4. 完善 removed-surface guard；
5. 更新 Cargo/lib 描述。

### R2 实施状态（2026-09-25）

**COMPLETE。**

- 新增 `src/command_spec.rs`，以 `CommandSpec / ActionSpec / OptionSpec` 统一描述顶层命令、子动作、usage、说明和公开 flags。
- 全局 `usage_text` 与 topic help 的命令/动作清单改由 `CommandSpec` 渲染；parser 继续独立承担值解析、互斥关系和安全语义校验。
- zsh/bash/fish 三种 Shell 补全均从同一命令目录注入顶层命令、各子动作与选项；R1 新增的 `--target`、`--partition` 已进入补全。
- completion 中已完全删除旧 `convert` 动作；顶层补全补齐 `tui`，并新增 schema/surface 门禁防止用户命令面再次漂移。
- `Cargo.toml` 与 `src/lib.rs` 产品描述从旧“免密转换”更新为“安全制盘”。
- R2 完整 CLI suite **63/63** 通过；completion 源码中 `convert`、旧硬编码顶层命令串与产品描述“免密转换”扫描均为 0。
- R2 fast 门禁 **6.08s / 0 failures**，full 门禁 **5.68s / 0 failures**；8 个非 HIL suite 与 doctest 全绿。

## Phase R3：删除明确死代码和旧产品兼容

优先高置信候选：

- wildcard_match
- read_lba_file
- backup_label_id
- backup_is_nopwd
- prepare_new_provision
- force_change_password_from_sectors
- `stale` convert completion

再盘点旧 EDPB 后处理 `.bin/.sha256` migration 链。

### R3 实施状态（2026-09-25）

**COMPLETE。**

- 删除无业务调用、仅由自测维持的 `wildcard_match`、`read_lba_file`、`backup_label_id`、`backup_is_nopwd`、`prepare_new_provision`、`force_change_password_from_sectors`，并清理对应辅助函数、导出与自测。
- 删除旧备份 SHA-256 旁挂文件读取入口 `sha256_sidecar_path/read_backup_sha256`；当前备份扫描、校验、删除与恢复统一依赖自包含 EDPB 容器及其内容摘要。
- 删除旧 `.bin`→EDPB 的写入/迁移入口 `write_legacy_migrated_backup` 以及仅服务该入口的写入分支；正式运行时只扫描 `.edpb`。
- 保留 `CaptureLevel::LegacyMigrated` 作为已经迁移成 EDPB 的历史清单反序列化值，并新增测试锁定这一只读兼容边界；真实 LBA0～12/LCE 协议兼容代码未删除。
- 备份与 EDPB 测试改为直接覆盖仍在使用的 `read_raw_protocol`、`lba4_label_id_from`、`image_is_nopwd` 等底层能力，不再通过已删除包装层自证存在。
- R3 定向验证 **239/239** 通过；上述高置信旧入口在 `src` 中扫描均为 0。
- R3 full 门禁 **24.58s / 0 failures**；重新编译后的首轮 fast 为 **48.61s**，随后暖缓存 fast **2.01s / 0 failures**，满足日常 `<45s` 目标。

## Phase R4：拆大模块

按边界逐步拆：

1. application/provision
2. diskio
3. tui/`state`
4. tui/render
5. tui/task

每次只移动一个领域，保持测试绿。

### R4 实施状态（2026-09-25）

**COMPLETE。**

- `application/provision.rs` 已按 prepare/commit/export 拆分，并将模块内测试迁出；门面文件由 2215 行降至 **579 行**。
- `diskio.rs` 已拆为 device/transaction/backup_config/backup_catalog/backup_create，门面文件由 1537 行降至 **264 行**；块设备事务与备份目录/配置职责分离。
- TUI 已按制盘、检查、备份、设备工作区拆分状态、渲染和任务逻辑，并将独立写盘任务拆到 `write_task.rs`；主状态文件 **1199 行**、主渲染文件 **811 行**、主任务文件 **660 行**。
- 新增 `architecture_split` 门禁，锁定领域模块存在、超大文件规模上限，并禁止各工作区的状态/渲染层直接依赖平台层或磁盘输入输出层。
- provision suite **176/176**、TUI suite **132/132**、R4 architecture gate **2/2** 通过；拆分只迁移职责边界，没有改变 LBA0～12/LCE 协议语义或写盘安全服务。
- R4 最终 fast 门禁 **5.59s / 0 failures**，full 门禁 **29.83s / 0 failures**，8 个非 HIL suite 与 doctest 全绿。

## Phase R5：`Inspect` 与语义层依赖收敛

1. 跨业务语义移入类型化协议层 `protocol::semantic`；
2. `metainfo` 改用类型化语义层；
3. `provision::validate` 改用类型化语义层；
4. `application::inspect` 消费检查展示适配结果并映射为应用层字段/视图；
5. `inspect.rs` 不再是其它业务领域的依赖中心。

### R5 实施状态（2026-09-25）

**COMPLETE。**

- 新增 `protocol::semantic` 类型化跨业务语义层，集中提供设备上下文、LBA6 SAFE6 文本、LBA7/LBA8/LBA11/LBA12 配置类型推断、LBA8 身份字段、PDKB `device_id` 与分区语义。
- `metainfo` 已移除对 `crate::inspect`、`SectorView` 和展示字段名称/分组的依赖，直接消费类型化语义。
- `provision::validate` 已移除对检查器展示模型的依赖；LBA4 直接调用规范解析器，LBA8/LBA11 通过语义层做往返语义验证。
- 检查器的 LBA7/LBA8/LBA11/LBA12 配置类型推断改为调用同一语义层，再映射为 `Field/View`；不再复制跨业务推断逻辑。
- 新增架构门禁：`metainfo.rs` 与 `provision/validate.rs` 禁止重新依赖 `crate::inspect`；`protocol::semantic` 禁止反向依赖展示层与应用层。
- R5 定向验证：检查套件 **53/53**、制盘套件 **176/176**；架构门禁同时锁定消费侧和语义层反向依赖。
- R5 最终 `fast` 门禁 **4.37s / 0 失败**，`full` 门禁 **9.30s / 退出码 0 / 0 失败**；macOS Plain `Virtual-HIL` **1/1** 通过，并完成弹出/重新挂载后的 exFAT 文件读回。真实 USB Phase 8 本轮未执行，仍保持未验收状态。

## Phase R6：进一步工程优化

- TargetSession
- EvidenceSource
- typed reports/events
- architecture `import` guards
- sccache
- test `profile` benchmark
- timing regression gate

### R6.1 `TargetSession` 实施状态（2026-09-25）

**COMPLETE。**

- 新增 `application::target_session` 类型状态安全会话，显式区分 `ReadOnly`、`PreparedWrite`、`WriteLocked`。
- Plain 制盘、Official 制盘与备份还原的真实写入口统一通过 `TargetSession` 完成 USB/系统盘保护、卸载/锁卷、读写重开与写前原始快照复核；平台 `WriteGuard` 生命周期由 `WriteLocked` 会话持有到写阶段结束。
- 制盘业务侧的硬件身份、容量、序列号、LBA3、协议读回与 transaction/readback/rollback 校验保持原逻辑，未降低安全门槛。
- 新增架构门禁，禁止 `application/provision/commit.rs` 与 `application/write.rs` 重新直接调用 `sysinfo::prepare_write` 或 `reopen_rdwr`。
- 定向测试：架构门禁 **1/1**、制盘套件 **176/176**、备份套件 **61/61** 全绿；`fast` 冷缓存 **52.27s / 0 失败**，同一工作区暖缓存复跑 **5.70s / 0 失败**。

### R6.2 `EvidenceSource` 实施状态（2026-09-25）

**COMPLETE。**

- 新增 `application::evidence`，统一物理盘与 EDPB 的只读证据入口，集中提供来源标签、总扇区数、LBA0～12 协议镜像、设备身份元数据、扇区读取与 EDPB `Artifact` 查找。
- `application::inspect` 不再自行打开物理裸盘或解析 EDPB 的 `Manifest`/`Artifact`；物理盘与 EDPB 均先构造 `EvidenceSource`，随后进入同一 `run_advanced_source` 解码路径。
- 原 `SectorReader` 契约迁入证据层并保持再导出，任意 LBA 的容量边界、未采集 EDPB 扇区报错、LCE/分区解码语义不变。
- 新增架构门禁，禁止 `application::inspect` 重新直接调用 EDPB 容器读取或 `FileDev::open_rdonly`。
- 定向测试：证据源架构门禁 **1/1**、检查套件 **53/53**、TUI 检查 **5/5** 全绿；`fast` 冷缓存 **48.44s / 0 失败**，暖缓存复跑 **4.24s / 0 失败**。

### R6.3 类型化报告/事件实施状态（2026-09-25）

**COMPLETE。**

- `WriteEvent` 保持为应用层的结构化进度事件，但 ANSI、颜色和 CLI 文本渲染已迁出应用层，由 `ui::render_write_event` 统一负责；`application` 不再写标准输出或依赖 `crate::ui`。
- CLI 的 `StdPrompter` 显式渲染事件，TUI worker 继续直接转发 `WriteEvent`；确认提示保留业务文本，并由 CLI 在前端恢复原有加粗样式。
- 新增 `BackupReport { path, is_nopwd }`，替代备份应用服务的匿名 `(PathBuf, bool)` 返回值；制盘已有 `ProvisionCommitReport/Outcome`，检查已有 `AdvancedInspectWorkspace`，跨前端结果边界均使用具名类型。
- 新增架构门禁，禁止 `application/write.rs` 重新依赖 `crate::ui` 或文本事件渲染，并禁止应用根模块直接向标准输出写入。
- 事件文本/ANSI 黄金基线与事件序列 **3/3**、CLI **63/63**、制盘 **176/176**、备份 **61/61**、TUI **132/132** 全绿；正式 `fast` **35.33s / 0 失败**。

### R6.4 架构导入门禁实施状态（2026-09-25）

**COMPLETE。**

- `architecture_split` 新增递归 Rust 源文件扫描，直接对领域层和应用层的导入方向做静态回归检查，不依赖人工审查。
- `src/provision/` 与 `src/protocol/` 禁止反向依赖 `crate::tui` / `crate::cli`；`src/application/` 与 `src/application.rs` 禁止依赖 `ratatui`、`crossterm`、`crate::tui`、`crate::cli`。
- `provision::validate` 不得依赖 `crate::inspect` 的展示模型继续由同一门禁锁定；已有 TUI 工作区→应用层、`protocol::semantic`→底层协议、`TargetSession`/`EvidenceSource` 边界门禁继续保留。
- 新增导入方向门禁 **1/1** 通过，当前代码无需为过门禁改写业务逻辑。

### R6.5 编译缓存实施状态（2026-09-25）

**COMPLETE。**

- `scripts/test-full.py` 在未显式设置 `RUSTC_WRAPPER` 时自动探测本机 `sccache`；存在时启用，缺失时明确提示并直接使用 `rustc`，不会把开发环境依赖变成测试前置条件。
- 使用 `sccache` 时自动补充 `CARGO_INCREMENTAL=0`，满足 Rust 编译缓存前提；CI 同样固定该变量。
- GitHub Actions 使用 commit SHA `fc920bf0ec8de6ee65d409111f7ec508035751ba` 固定 `mozilla-actions/sccache-action` v0.0.11，并固定跨现有 CI 平台已确认发布资产的 `sccache` v0.17.0。
- 本机通过 Homebrew 安装并验证 `sccache 0.18.0`。同一 target 路径清空产物后重建：首次 **14.88s**，缓存复用重建 **8.93s**；累计 Rust 命中 **71** 次，命中率 **49.31%**。
- 缺少缓存程序路径也已验证：运行器输出降级提示后正常完成，不改变 fast/full 正确性门禁。

### R6.6 测试性能基准实施状态（2026-09-25）

**COMPLETE。**

- 新增 `scripts/test-benchmark.py`，只编排现有 `scripts/test-full.py`，不复制测试组选择、Cargo 编译或测试执行逻辑。
- 支持 `--profile fast|full|both`、`--repeat N` 与 `--json PATH`；每个测试档位输出独立样本，并汇总最小值/中位数/最大值，可用于人工审计与后续自动门禁。
- 完整性能基准使用不少于 900 秒的持久任务执行，避免再次落入 120 秒同步调用限制。
- 本机 3 次暖缓存基线：`fast` **1.39 / 1.52 / 1.69s**，中位数 **1.52s**；`full` **8.90 / 9.16 / 9.85s**，中位数 **9.16s**；全部 0 失败。
- 机器可读 JSON 已验证生成，便于后续趋势统计与耗时回归门禁消费。

### R6.7 耗时回归门禁实施状态（2026-09-25）

**COMPLETE。**

- `scripts/test-full.py` 新增 `--max-seconds`，并支持环境变量 `EDPCLI_TEST_MAX_SECONDS`；功能失败优先报告，功能全部通过后才检查总耗时预算。
- `scripts/test-fast.sh` 默认传入 **45 秒**预算，可用 `EDPCLI_FAST_MAX_SECONDS` 显式覆盖；GitHub Actions 的完整非 HIL 门禁设置 **120 秒**性能预算。该预算只判断性能回归，不是同步命令执行超时。
- 静态基础设施门禁 **1/1** 通过；负向运行以 **0.001 秒**预算执行时，测试本身全部通过后明确输出 `timing budget exceeded` 并以 **exit 1** 结束，证明门禁实际生效。
- 正式 `scripts/test-fast.sh` 在默认 45 秒预算下通过；最终已提交代码重跑为 **41.27s / 0 失败 / 退出码 0**，当前性能满足门槛。

### Phase R6 总体实施状态（2026-09-25）

**COMPLETE。**

- R6.1～R6.7 已全部完成：`TargetSession`、`EvidenceSource`、类型化报告/事件、架构导入门禁、`sccache`、测试性能基准、耗时回归门禁均已落地并具备自动回归检查。
- 最终代码门禁：`cargo fmt --all -- --check` 与 `git diff --check` 通过；`fast` **41.27s / 0 失败 / 退出码 0**。
- 最终完整非 HIL 门禁通过持久任务执行：8 个正式测试组 + doctest，**39.53s / 0 失败 / 退出码 0**，并显式应用与 CI 一致的 **120 秒性能预算**。
- macOS Plain Virtual-HIL **1/1 通过 / 退出码 0**；完成虚拟盘制盘、重新识别、弹出/重新挂载与 exFAT 文件读回。`hdiutil attach -nomount` 仅产生弃用警告，不影响结果。
- 本轮未执行真实 USB Phase 8；其真实硬件验收仍明确保持 **未执行/未验收**，没有被架构重构或虚拟 HIL 结果替代。

## Phase D0：测试聚合、Clippy 与失效迁移入口收口

**COMPLETE。**

- 全量技术债复审发现，R0 将 integration tests 聚合为 suite 后，多个子测试仍各自 \`mod common\` / \`mod gold_name\`，导致 \`cargo clippy --all-targets --locked -- -D warnings\` 触发 \`duplicate_mod\` / \`duplicated_attributes\`；同时两处普通测试 lint 也会使发布门禁失败。
- 六个聚合 suite 现在各自只装载一次共享 \`common\`，需要历史金标文件名适配的 suite 也只装载一次 \`gold_name\`；子测试通过 suite 根模块复用共享支持代码，避免同一源文件在一个 crate 中重复编译。
- 删除已经失效的 \`examples/migrate_legacy_backups.rs\`。该 example 仍调用 R3 已删除的 \`read_backup_sha256\` / \`write_legacy_migrated_backup\`，属于明确的旧 \`.bin/.sha256\` 产品迁移残留；\`dead_code_guard\` 新增约束，禁止该入口再次出现。
- \`scripts/test-fast.sh\` 已把 \`cargo clippy --all-targets --locked -- -D warnings\` 纳入仓库自有日常门禁，\`tests/test_infrastructure.rs\` 锁定这一契约，避免以后出现“fast/full 测试绿但正式 Clippy 红”的分裂状态。
- 修复相关测试自身的 \`manual_contains\`、\`unnecessary_get_then_check\` 与 \`field_reassign_with_default\`；当前全目标 Clippy 已实际以 **exit 0 / 0 warnings** 通过。
- 本阶段只治理测试基础设施与已废弃产品入口，没有修改 LBA0～12/LCE 协议语义、真实介质兼容解析或任何写盘安全门槛。

## Phase D1：TUI 后台任务槽收敛

**COMPLETE。**

- `TaskHub` 原有 9 组 `GenerationGate + SingleFlightGate` 平铺字段已收敛为统一泛型 `TaskSlot<P>`；设备扫描、备份扫描和备份校验通过 `pending_latest` 只保留最新请求，全盘检查、制盘计划、备份清理、批量删除等拒绝并发的任务使用同一任务槽的 `try_begin/finish` 生命周期。
- `GenerationGate` / `SingleFlightGate` 的既有公开行为保留，现有并发契约测试继续覆盖过期结果丢弃与单任务执行；新增源码契约锁定 `device_slot / backup_slot / verify_slot`，禁止重新长回成成对平铺字段。
- 关键写盘 `active_operation + critical_worker` 完全保持独立，没有进入普通后台任务槽；终端异常时等待关键工作线程完成读回与回滚安全链的行为未改变。
- 定向 `tui_suite` **161/161** 通过；全目标 Clippy **0 条警告**；包含 Clippy 的正式快速门禁 **退出码 0 / 约 41s**。
- 本阶段只改变后台任务编排结构，不改变设备扫描、备份、全盘检查、制盘的业务结果，也不改变任何真实盘写入事务边界。

## Phase D2：EDPB 完整性状态去历史旁挂语义

**COMPLETE。**

- 备份运行时早已收敛到自包含、自校验的 EDPB，但类型仍保留旧旁挂文件时代的 `Sha256Status::NoSidecar`。该状态在现行扫描路径不可达，且让应用层、命令行和 TUI 继续暴露“缺 SHA-256”这一过时概念。
- 统一改为 `BackupIntegrityStatus::{Verified, Invalid}`，`BackupEntry` 与 `BackupWorkspaceItem` 对应字段统一为 `integrity_status`；命令行和 TUI 只呈现 EDPB 完整/损坏状态，不再假设存在外部摘要旁挂文件。
- `scan_backup_names` 的过时 `.bin` 注释已修正为现行 `.edpb` 轻量索引语义；内容摘要字段仍保留用于删除前同名替换防护，它与旧旁挂文件无关。
- 全仓源码扫描确认 `Sha256Status`、`NoSidecar`、`sha256_ok`、`sha256_status` 已清零；真实介质协议兼容与历史 EDPB 清单只读兼容未改。
- 定向 `backup_suite` **61/61**、`tui_suite` **161/161** 通过。

## Phase D3：TUI 裸盘访问边界收口

**COMPLETE。**

- 新增应用层只读整盘打开入口，并在备份、还原与制盘应用服务中提供面向前端的整盘操作封装。TUI 不再自行拼接裸盘路径、构造 `FileDev`、持有 `SystemClock` 或直接调用平台层。
- 制盘计划、制盘提交、制盘前备份、普通备份与还原均由 TUI 只传递业务请求、固定身份与目标路径；只读打开、USB/系统盘保护和选择后身份复核由应用层统一执行。
- 写入服务内部原有 `TargetSession`、卸载/锁卷、读写重开、写前快照复核、原子写、逐扇区读回与回滚链保持原样；本阶段只移动资源获取职责，没有降低任何安全门槛。
- 新增全 TUI 递归架构门禁，禁止 `src/tui/` 重新引用 `crate::platform`、`crate::diskio`、`FileDev::open_rdonly`、`raw_path(` 或 `SystemClock`；备份创建契约同步锁定应用层整盘入口。
- 源码扫描确认上述 TUI 直接依赖均为 **0**。

## Phase D4：发布门禁与依赖供应链治理

**COMPLETE。**

- 发布工作流与未来兼容工作流不再直接执行旧的 `cargo test --all-targets`，统一调用仓库自有 `scripts/test-full.py --profile full`；发布元数据、Clippy 与构建命令全部使用锁文件，避免发布路径与主 CI 的依赖解析和测试口径漂移。
- 新增 `deny.toml` 与固定版本 `cargo-deny 0.20.2` 的独立供应链门禁，检查 RustSec 公告、许可证、依赖来源和通配版本要求；该工具只作为 CI/显式审计工具，不成为本地快速门禁的安装前置条件。
- 第一次真实执行供应链检查发现 `ratatui 0.29.0` 唯一引入已停止维护的 `paste 1.0.15`（RUSTSEC-2024-0436）。没有添加忽略项，而是升级到 `ratatui 0.30.2`，并将直接 `crossterm` 升到 `0.29.0`，使 `paste` 清零且终端后端只保留一个 crossterm 版本。
- 新增 Dependabot 周期更新，分别跟踪 Cargo 与 GitHub Actions；架构测试锁定发布/兼容工作流必须继续使用仓库全量门禁、锁依赖，并锁定供应链检查与自动更新配置存在。
- 实际验证：`cargo deny check` 的公告、禁止项、许可证和来源检查均为 **通过**；终端依赖升级后 `repository_suite` **30/30**、`tui_suite` **161/161**；全目标 Clippy **0 条警告**；快速门禁暖缓存 **10/10 产物、4.75s**；全量门禁 **8 suites / 10 产物 / doctest，7.56s**。
- 供应链检查继续以警告形式暴露 `ratatui` 内部无法由本项目直接统一的 `hashbrown` / `syn` 多版本，不用全局允许项隐藏；一旦上游依赖树收敛，门禁会自然反映变化。

## Phase D5：CLI 裸盘访问边界收口

**COMPLETE。**

- CLI 保留命令解析、交互确认、提权与目标选择职责，但不再自行拼接裸盘路径、构造 `FileDev`、持有 `SystemClock` 或组装写入业务上下文。Provision 计划/镜像/写入、备份创建与还原均改为调用 application 的整盘服务入口。
- 备份目录解析、配置存在性判断以及提权 argv 的目录桥接也经 application 暴露，避免为了非业务细节重新引入 `crate::diskio`。
- 原先寄居在 `cli.rs` 的两条 `read_image`/伪 `SectorDev` 测试迁回 `application/write.rs`；相关集成测试改为直接引用 `application::write::{Ctx, backup_create_flow, restore_flow}`，CLI 不再充当 application API 转发层。
- 新增架构门禁，禁止 `src/cli.rs` 出现 `crate::diskio`、`FileDev::open_rdonly`、`raw_path(` 或 `SystemClock`，并锁定四个整盘应用入口 `prepare_provision_on_disk / commit_provision_on_disk / backup_create_on_disk / restore_on_disk`。
- 源码扫描确认上述 CLI 直接裸盘依赖全部为 **0**；写盘事务内部的 `TargetSession`、USB/系统盘保护、写前备份、卸载/锁卷、重开身份复核、原子写、读回与回滚逻辑均未改变。
- 定向 `backup_suite` **61/61**、`cli_suite` **63/63**、`provision_suite` **178/178**、`repository_suite` **31/31**，共 **333/333** 通过。

## Phase D6：应用错误模型类型化

**COMPLETE。**

- `application::evidence` 新增 `EvidenceError`，将 EDPB 校验/协议读取/长度/几何信息与物理盘目标、几何信息、只读打开、协议读取失败改为类型化错误，不再以裸 `String` 承担应用控制流。
- `application::inspect` 新增 `InspectError` / `InspectErrorKind::{InvalidRequest, OutOfRange, Backup, Io, Decode, Target}`；LBA 请求解析、decoder、任意扇区读取、越界、导出等路径统一返回类型化错误。CLI 按 `InspectErrorKind` 映射退出码，禁止再通过本地化错误文本的 `message.contains(...)` 推断控制流。
- 备份删除链新增 `DeletePlanError`、`DeleteExecuteError` 与 `BackupDeleteError`，测试直接断言错误 variant；TUI 只在 worker/消息边界调用 `.to_string()`，应用层保留结构化错误。
- Windows 测试基础设施同步治理：Provision 稀疏镜像测试临时文件改用安全 ASCII stem + process id + atomic sequence，不再把包含 `::` 的 Rust test name 拼入文件名；`tui_keymap_contract` 改为函数/阶段区间内的关键 token 顺序契约，不再依赖 CRLF、缩进或 rustfmt 的多行布局。
- 最终 PR #18 head 的 Rust CI 六个平台（macOS arm64/x86_64、Linux arm64/x86_64、Windows arm64/x86_64）均通过 Rustfmt、正式 full、Clippy `-D warnings` 与 release build；checked-in protocol gold audit、dependency policy/cargo-deny 与四平台 Virtual Disk HIL 全部通过。
- 本阶段未改变 LBA0～12/LCE 已闭环协议语义，也未降低 `system-disk guard`、`USB guard`、写前备份、`unmount/lock`、`reopen identity`、`atomic write`、`readback` 或 `rollback`。Virtual HIL 仍不替代真实 USB：Chapter 11 实盘只读检查验收与旧 Phase 8 真实 USB 破坏性写入 HIL 继续保持未最终验收状态。

## Phase D7-A：fast runner 测试源归属完整性

**COMPLETE。**

- 审计发现 8 个正式 non-HIL suite root 实际包含 **75** 个顶层测试源，而 `scripts/test-full.py` 的手工 `TEST_SOURCE_SUITES` 只有 **49** 项，缺少 **26** 项；包括 `command_spec.rs`、`inspect_backup_workspace.rs`、repository suite 的多个契约测试以及整组 TUI 测试。此前这些文件依赖路径前缀 fallback 或核心 suite 兜底，存在新增测试后 fast selection 静默漂移的维护风险。
- 删除 49 项手工映射事实源；新增 `discover_test_source_suites()`，直接从 8 个 suite root 的 `#[path = "..."]` 声明派生普通顶层 `tests/*.rs` → suite 映射。共享 `tests/common` / `tests/support` helper 不参与 fast selection，重复归属或 suite 引用不存在的顶层测试会立即失败。
- `repository_suite` 新增归属完整性门禁：除 8 个 suite root 与两个显式 Virtual-HIL target 外，每个普通顶层 `tests/*.rs` 必须恰好属于一个 suite；suite 中的每个顶层引用也必须真实存在。以后新增测试文件若未接入 suite，fast/full 均会由 repository gate 明确失败，而不是静默退化。
- 测试先行分两步提交：先增加 ownership/runner 契约门禁，再实现自动派生映射。最终 PR #19 head 六平台 Rustfmt/full/Clippy/release、checked-in protocol gold audit、dependency policy/cargo-deny 与四平台 Virtual Disk HIL 全部通过。
- 本阶段只治理测试基础设施，不改变 LBA0～12/LCE 协议、Provision 业务语义或任何真实盘写入安全门槛。

## Phase D7-B1：Provision form/capacity/plain model 拆分

**COMPLETE；D7-B 继续。**

- `src/tui/provision/state.rs` 原为 **2667 行**，同时承载 Provision 阶段编排、Official/Plain 表单数据、容量单位换算、Plain 分区表单、输入策略、字段编辑、布局预览与确认/执行状态，职责仍过密。
- 新增 `src/tui/provision/form.rs`，集中承载 `ProvisionForm`、`PlainPartitionForm`、`PlainProvisionForm`、`MiB`/`GiB`/`sector` 精确容量换算、容量单位切换、输入策略与文件系统切换。原公开表单类型继续由 `tui::state` 重新导出，对外接口不变；仅父级编排模块确实需要的内部状态使用 `pub(super)`。
- `state.rs` 降至 **2199 行**，`form.rs` **474 行**。新增架构门禁要求 `state.rs < 2250`、`form.rs < 650`，并禁止表单模型依赖 `AppState`、`crate::application`、`crate::platform` 或 `crate::diskio`，防止纯表单层重新吸收编排与输入输出责任。
- 该拆分只移动纯 TUI 状态/换算逻辑；`ProvisionState` 阶段编排、事件循环、application Provision 服务以及 system-disk guard、USB guard、写前备份、unmount/lock、reopen identity、atomic write、readback、rollback 全部保持原路径。
- D7-B 后续继续拆分 Plain 分区编辑、字段导航与输入、校验与复核、布局展示等职责；本小阶段完成不代表 Provision 状态治理全部结束。

## Phase D7-B2.1：Plain 分区编辑职责拆分

**COMPLETE；D7-B 继续。**

- 新增 `src/tui/provision/plain_editor.rs`，承载 Plain 字段槽解析、表单到 `PlainProvisionPlan` 的转换、容量填满、容量单位与文件系统切换、分区增删。`state.rs` 只保留目标盘容量读取、消息与光标同步等工作区编排。
- Plain 继续使用独立的 `ProvisionTarget::Plain`，分区布局、容量上界与计划合法性仍调用 `crate::provision` 的规范实现；没有引入 mode4 或复制协议解析器。
- 架构门禁要求 Plain 编辑模块存在、保持小规模，并禁止依赖 `AppState`、application、platform 或 diskio；`state.rs` 上限进一步收紧为 2160 行。
- `state.rs` 为 **2147 行**、`form.rs` 为 **447 行**、`plain_editor.rs` 为 **139 行**。TUI suite **161/161**、架构测试、fast **4.66s / 0 失败**、full **33.25s / 0 失败**、全目标 Clippy、rustfmt 与 diff 检查通过。首次冷缓存 fast 功能测试全过，但 **48.06s** 超过 45s 性能预算；暖缓存正式复跑通过。
- 本阶段没有修改 LBA0～12/LCE、设备写入路径或写盘安全门槛；真实 USB HIL 仍待独立验收。

## Phase D7-B2.2：字段导航与输入编辑拆分

**COMPLETE；D7-B 继续。**

- 新增 `src/tui/provision/fields.rs`，集中管理 Official/Plain 字段槽与可见行映射、字段分组和紧凑布局、选中字段读取、光标移动、输入策略、字符插入、退格与删除。现有 `AppState` 方法签名与 TUI 键位调用保持原样。
- `state.rs` 为 **1490 行**、`fields.rs` 为 **662 行**。架构门禁要求字段模块存在、保留关键方法且不回迁到编排文件，并收紧 `state.rs < 1500`、`fields.rs < 900`。
- TUI suite **161/161** 与架构测试通过；fast 暖缓存 **4.50s / 0 失败**、full **32.40s / 0 失败**，全目标 Clippy、rustfmt 与 diff 检查通过。首次冷缓存 fast 功能测试全过，但 **47.38s** 超过 45s 性能预算；暖缓存正式复跑通过。字段移动没有修改业务验证、协议解析或写盘服务。后续继续拆出表单请求转换与布局展示。

## Phase D7-B2.3：预填解析、容量边界与请求适配拆分

**COMPLETE；D7-B 继续。**

- 新增 `src/tui/provision/validation.rs`，集中承载目标模式适配、现有配置预填解析、容量边界计算以及 Official 表单到 `OfficialProvisionRequest` 的转换。Plain 表单计划转换保留在独立的 `plain_editor.rs`。
- LCE 几何、预填、分区几何与容量上界继续调用 `crate::protocol` 和 `crate::provision` 规范实现；TUI 只保留目标盘与表单适配，不新增协议解析器或写盘验证真相源。
- `state.rs` 为 **1105 行**，`validation.rs` 为 **394 行**。架构门禁要求适配模块存在、保持小规模，并收紧 `state.rs < 1200`。
- TUI suite **161/161**、Provision suite **178/178** 与架构测试通过；fast 暖缓存 **4.53s / 0 失败**、full **33.06s / 0 失败**，全目标 Clippy、rustfmt 与 diff 检查通过。首次冷缓存 fast 功能测试全过，但 **46.89s** 超过 45s 性能预算；暖缓存正式复跑通过。后续拆分布局展示。

## Phase D7-B2.4：布局展示拆分

**COMPLETE；D7-B 继续。**

- 新增 `src/tui/provision/layout.rs`，集中承载几何预览、Plain 与 Official 布局编辑行、布局模型和容量条展示。展示层继续消费 `plain_form.plan`、`provision_resolved_prefill` 与 `crate::provision` 几何校验，不生成新的协议或分区真相源。
- `state.rs` 为 **739 行**、`layout.rs` 为 **371 行**；架构门禁要求布局模块存在、保持小规模，并收紧 `state.rs < 800`。
- TUI suite **161/161** 与架构测试通过；fast 暖缓存 **4.56s / 0 失败**、full **34.45s / 0 失败**，全目标 Clippy、rustfmt 与 diff 检查通过。首次冷缓存 fast 功能测试全过，但 **48.05s** 超过 45s 性能预算；暖缓存正式复跑通过。后续收口表单编辑动作，使 `state.rs` 只保留工作区阶段与任务协调。

## Phase D7-B2.5：表单编辑动作收口

**COMPLETE；D7-B 完成。**

- 新增 `src/tui/provision/editor.rs`，集中承载容量填满、选项切换、Plain 计划与分区增删、密码选项切换等表单交互动作。纯 Plain 编辑规则仍由 `plain_editor.rs` 执行；`editor.rs` 负责消息、选中字段和光标同步。
- `state.rs` 降至 **501 行**，保留 Provision 工作区初始化、备份提示、阶段转换、导出确认与写盘任务协调；`editor.rs` 为 **243 行**。架构门禁收紧为 `state.rs < 520`、`editor.rs < 300`，并锁定编辑入口不回迁。
- TUI suite **161/161** 与架构测试通过；fast **4.60s / 0 失败**、full **32.70s / 0 失败**，全目标 Clippy、rustfmt 与 diff 检查通过。首次冷缓存运行发现本报告中一处英文叙述词，被文档门禁拒绝；修正后正式复跑通过。D7-B 的表单、Plain 编辑、字段输入、校验适配、布局展示与编辑动作均已按职责分离；后续进入 D7-C `inspect.rs` 拆分。

---

# 第八部分：完成标准

本轮重构只有同时满足以下条件才算完成：

1. CLI/TUI 五种制盘目标业务能力一致；
2. Plain 是一等 ProvisionTarget，不是 mode4；
3. parser/help/completion 单一事实源；
4. Offline Convert 不再出现在任何用户 completion/help；
5. integration test crate 数量显著下降；
6. fast gate 本机约 45 秒内稳定；
7. full gate 不依赖 120 秒同步调用；
8. full gate 有明确最终 exit code；
9. TUI God `state`/render 按 workspace 拆分；
10. application provision official/plain/commit/export 职责清晰；
11. block I/O 与 backup catalog/config 分离；
12. metainfo/provision `validator` 不依赖 `Inspect` presentation model；
13. 高置信死代码删除；
14. 旧 `.bin` 迁移写入链按实际历史数据完成清理；
15. 真实协议 `legacy` parser 未被误删；
16. `ARCHITECTURE.md` 与实现一致；
17. Cargo/lib 产品描述不再称 Offline Convert 为现有能力；
18. fmt/diff/fast/full/Virtual-HIL 全绿；
19. Phase 8 剩余真实 USB 验收另行继续，架构重构不得降低写盘安全门槛。

---

# 第九部分：给实施 AI 的硬约束

实施时必须：

- 开始先读 `AGENTS.md` 和本报告；
- 重新检查 `git status/HEAD/log`，现实状态优先；
- 禁止 `git reset/clean`；
- **先完成 R0，再开始大规模重构**；
- 测试先行、小步提交、及时 push；
- 不改变已闭环 LBA0～12/LCE 协议语义；
- 不以 `legacy` 字符串为删除依据；
- 不复制新的 CLI/TUI `writer`/backend；
- 每次删除 API 前先全仓检索；
- 所有 write path 保持 USB/system-disk guard、backup、reopen identity、transaction/readback/rollback；
- 不把 Phase 8 未完成的真实 USB 场景写成通过。

本报告是本轮架构审计和实施计划的事实源；执行中发现现实代码变化时，应更新本报告的实施状态，不另建平行计划。
