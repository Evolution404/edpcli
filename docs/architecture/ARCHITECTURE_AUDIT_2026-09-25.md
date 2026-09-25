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

## Phase R1：CLI/TUI 能力对齐

1. 抽统一 `ProvisionRequest::{Official,Plain}`；
2. CLI 增加 Plain plan/write；
3. 明确并实现 Plain image/export 策略；
4. 两端共用 application prepare/commit；
5. 建 capability parity test。

## Phase R2：命令单一事实源

1. 建 CommandSpec；
2. help/completion 从统一 schema 生成；
3. 删除 completion `convert`；
4. 完善 removed-surface guard；
5. 更新 Cargo/lib 描述。

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

## Phase R4：拆大模块

按边界逐步拆：

1. application/provision
2. diskio
3. tui/`state`
4. tui/render
5. tui/task

每次只移动一个领域，保持测试绿。

## Phase R5：`Inspect` `semantic` dependency 收敛

1. 跨业务语义移入 typed protocol `semantic`；
2. metainfo 改用 typed `semantic`；
3. provision `validator` 改用 typed `semantic`；
4. application::`inspect` 映射为 Field/View；
5. `inspect`.rs 不再是其它 domain 的依赖中心。

## Phase R6：进一步工程优化

- TargetSession
- EvidenceSource
- typed reports/events
- architecture `import` guards
- sccache
- test `profile` benchmark
- timing regression gate

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
