# EDP LBA0–LBA12 协议工程化实施计划

> 状态基线：LBA0–LBA12 共 6656B 的 EDP 字节语义已完成 6656/6656B = 100.0% 闭环。
>
> 本计划的目标不是继续堆叠逆向笔记，而是把既有结论工程化为：
> 1. 人类可直接阅读的字段手册；
> 2. 可被 Rust 代码直接消费的协议模型；
> 3. 覆盖 LBA0–LBA12 每个字节、每个已知 profile 的结构化测试；
> 4. 完全纳入 edpcli 仓库、clean clone 可重放且不依赖外部 `u_disk` 目录的证据与测试体系。


## 0. 2026-09-22 当前执行状态

### 0.1 仓库与 main 合并状态

当前工作分支：

```text
feat/provision-new-usb-20260919
```

已将 `origin/main@d4bad92`（v2.3.0）合入当前分支，merge commit：

```text
4103223 Merge remote-tracking branch 'origin/main' into feat/provision-new-usb-20260919
```

合并时没有使用 `-X ours` / `-X theirs` 全局覆盖。协议证据链、v2.3.0 TUI/动画/任务串行化、provision/inspect/diskio 等冲突按语义逐项合并。合并完成后全仓测试：

```text
460 / 460 PASS
```

后续完成 LBA7/8/10/11/12 typed protocol 后，全仓测试提升为：

```text
469 / 469 PASS
```

当前已提交 HEAD：

```text
9fe3771dd58f0f6c8a3f58549951f9671751de37
feat: type LBA11 capacity profiles
```

### 0.2 字段级协议工程化已经 142/142

当前 `field_catalog.tsv` 的工程状态已经达到：

```text
LBA0-LBA12 semantic coverage        = 6656 / 6656 B
field × profile rows                = 142
implementation COMPLETE             = 142 / 142
behavior-test COMPLETE              = 142 / 142
```

最后五个 sector 的提交：

```text
e67bfda  feat: type LBA7 EDPF and pass-info profiles
268c3e9  feat: type LBA12 wrapped-key profiles
082af6f  feat: type LBA8 dynamic label profiles
49933bc  feat: type LBA10 EESI profiles
9fe3771  feat: type LBA11 capacity profiles
```

已完成的关键 typed 行为包括：

- LBA7：two-entry / three-entry、PassInfo v0x0064 / v0x0206；
- LBA8：动态 encrypted_len、current / transitional / strict-legacy UsbOnlyInfo、HostHardinfo 独立 axis；
- LBA10：absent-zero / EESI-enabled，且 +0x80..1FF tail preserve；
- LBA11：exact DiskSize / repair-CHS 两种容量派生；
- LBA12：外层 sector cipher 与 entry 内 wrapped-key algorithm 分离建模；mode1/2/3/legacy profile 独立；
- 所有上述 parser 都要求显式 profile，Unknown fail-closed，不使用“不是 A 就当 B”的 fallback。

### 0.3 LBA12 coverage 账本修正

本轮发现并修正了一处旧账本混淆：

```text
PassInfo.Version == 0x0064
```

不能当成：

```text
LBA12 wrapped-key mode == legacy mode0
```

当前 checked-in 物理 LBA12 corpus 中，`NeedEncrypt != 0` 的 entry 实际正例是 mode2；
mode1/mode3 有 first-party virtual writer 正例；legacy mode0 compatibility 语义闭环，但当前没有 committed positive physical LBA12 capture。

因此当前 WIP 已同步修正：

- `audit/protocol/field_catalog.tsv`
- `audit/protocol/profile_axes.tsv`
- `audit/protocol/profile_coverage.tsv`
- `docs/EDP_LBA0_12_FIELD_GUIDE.md`

不得再把 PassInfo 版本和 wrapped-key profile 混为同一个 axis。

### 0.4 当前未提交 WIP

当前正在实现 Phase 3 最后一部分：

```text
src/protocol/image.rs
```

目标接口：

```rust
parse_protocol_image(
    raw: &[u8; 6656],
    context: ProtocolImageContext,
) -> Result<ProtocolImageView, ProtocolError>
```

`ProtocolImageContext` 显式携带：

- `ProtocolProfile`
- `device_id`
- `VID/PID`
- 物理容量

不做无上下文“猜 profile”。

当前 WIP 已加入的跨 LBA invariant 包括：

- LBA6 device CRC == `CRC32(device_id)`；
- LBA4 HostHardinfo == LBA8 HostHardinfo；
- LBA6/LBA9 Dept profile 一致并可恢复完整 Dept/User；
- LBA7 / LBA12 EDPF entry 数量一致；
- LBA7 / LBA12 partition type 顺序一致；
- legacy LBA6 MBR snapshot 与 LBA12 type4 geometry 一致；
- LBA11 PDKB UID == 当前 `device_id`；
- profile 与实际 LBA0/GPT/LBA3/LBA6/LBA9 状态必须一致。

截至本状态记录，WIP 已通过：

```text
cargo check --all-targets              PASS
protocol_field_catalog                26 / 26 PASS
protocol_field_guide                   2 / 2 PASS
```

但 **尚未提交**，且还没有把整镜像正例/破坏样本 cross-LBA tests 补齐，因此不得把 Phase 3 整体标记完成。

当前工作区还包含：

```text
?? audit/protocol/independent_review_2026-09-22.md
```

该独立审计文档继续保持未跟踪、未修改；除非用户明确要求，不纳入提交。

### 0.5 剩余工程工作与执行顺序

字段级 142/142 后，剩余工作严格按以下顺序执行：

1. **完成整镜像 parser / cross-LBA validation**
   - 使用 committed 6656B physical gold 做正例；
   - 添加至少一个破坏 LBA7/LBA12、LBA6/LBA12、device_id consistency 的负例；
   - 整镜像重建必须保持 13 个 sector wire bytes 可验证。

2. **完成 external runtime dependency gate**
   - 日常 `cargo test` 不允许访问 `/Users/.../u_disk`、`~/Desktop/u_disk`、`/private/tmp`；
   - 文档中的 provenance 字符串可以保留；
   - `scripts/protocol` 中仍需手工外部二进制的研究脚本必须改成显式参数，不得作为默认运行时依赖。

3. **生成自动 coverage report**
   - 输出 JSON + Markdown；
   - 至少统计 13 LBA、6656B ownership、142 rows、profile states、parser/test/evidence、physical/virtual modality；
   - physical gap 不得被 virtual positive 偷换。

4. **README / AGENTS 协议入口**
   - 日常开发从 FIELD_GUIDE / src/protocol / field_catalog / coverage tests 进入；
   - 逆向总文档只作为 provenance/history。

5. **最终门禁**
   - `cargo fmt --all -- --check`
   - `cargo test`
   - `git diff --check`
   - clean-clone protocol tests
   - worktree 除明确保留的独立审计文档外 clean
   - 提交并 push

6. **完成上述协议工程收口后，再进入正式制盘重构**
   - 禁止让旧 `ProvisionProfile::canonical_v1()` 继续代表所有官方模式；
   - 新 builder 必须反过来通过 `ProtocolImageView` / cross-LBA validator 验证，禁止 producer 自证。

## 1. 总体原则

### 1.1 一个事实，三种表达

协议事实只允许有一个语义来源，但要同步投影为三种形式：

- **人类可读**：`docs/EDP_LBA0_12_FIELD_GUIDE.md`
- **机器可读**：`audit/protocol/field_catalog.tsv`
- **代码可执行**：`src/protocol/`

测试负责证明三者一致，禁止文档、账本、代码各自维护一套互相漂移的 offset 和语义。

### 1.2 不建立伪造的“线性协议版本号”

现有证据表明，历史差异不是简单的 v1 → v2 → v3：

- 同一年代可以同时存在不同 wire profile；
- 不同 LBA 的演进轴可以彼此独立；
- join59、HSerial、MBR underlay、LBA0 bootstrap 等历史特征不能假设必然同时出现；
- reader 支持某 profile，不代表同版本 writer 一定生成该 profile。

因此协议建模采用：

**稳定字段语义 + 正交 profile 轴 + 实现 provenance**

而不是一个巨大的 `Legacy2019` / `Legacy2022` 枚举。

### 1.3 “语义闭环”和“实现来源”继续分离

字段可以在 EDP wire semantics 上 COMPLETE，同时保留 implementation provenance 问题。例如：

- exact historical join59 writer EXE 未取得；
- legacy MBR snapshot 的 exact copy-site/profile selector 未取得；
- GPT enabled、LBA12 mode1/mode3 缺真实物理正例。

这些必须继续记录，但不能重新混入字节语义状态。

---

## 2. 历史差异如何记录

### 2.1 使用正交 profile 轴

建议定义：

```rust
pub struct ProtocolProfile {
    pub lba0_bootstrap: Lba0BootstrapProfile,
    pub dept_layout: DeptProfile,
    pub lba4_encoding: Lba4EncodingProfile,
    pub lba10_eesi: EesiProfile,
    pub lba11_capacity: Lba11CapacityProfile,
    pub lba12_layout: Lba12Profile,
}
```

典型枚举：

```rust
pub enum DeptProfile {
    Short,
    Join59,
    Join60,
    Unknown,
}

pub enum Lba0BootstrapProfile {
    Zero,
    UsbMainBSec,
    NetacMbr,
    Unknown,
}

pub enum Lba12Profile {
    LegacyV0064,
    Mode1,
    Mode2,
    Mode3,
    Unknown,
}
```

关键要求：

- 每个 profile 轴独立判定；
- 禁止用一个历史软件版本推导其它 LBA 的 profile；
- 检测失败必须进入 `Unknown` / `Unsupported`，禁止“不是 A 就默认 B”。

### 2.2 同一 offset 在不同 profile 下允许不同语义

例如 LBA6 `+0x03F`：

| Profile | 含义 |
|---|---|
| short | Dept NUL 后 backing |
| join59 | join discriminator / NUL |
| join60 | Dept[59] |

LBA9 `+0x080..+0x0FF`：

| Profile | 含义 |
|---|---|
| short | preserve / unowned backing |
| join59 | Dept continuation，从 Dept[59] 开始 |
| join60 | Dept continuation，从 Dept[60] 开始 |

因此 `field_catalog.tsv` 不采用“一行一个 offset”，而采用：

**一行一个 field × profile state**。

### 2.3 历史变化类型显式分类

建议统一成：

```rust
pub enum EvolutionKind {
    AddedRemoved,
    EncodingChanged,
    OwnershipChanged,
    ProducerChanged,
}
```

含义：

- `AddedRemoved`：某代字段存在，另一代不存在；
- `EncodingChanged`：逻辑字段相同，wire representation 不同；
- `OwnershipChanged`：同一 offset 在不同 profile 下由不同字段/区域拥有；
- `ProducerChanged`：盘面语义不变，但生成算法或 producer 路径不同。

### 2.4 软件版本只做 provenance，不直接决定 wire profile

字段记录中必须分开：

- wire profile；
- observed physical profile；
- producer implementation；
- consumer implementation；
- software version/build/hash；
- exact historical producer 是否已取得。

例如 CEMS2.0 FileOpHook 可以证明 join59 reader semantics，但不得因此被标成 join59 writer。

---

## 3. 目标仓库结构

```text
src/protocol/
  mod.rs
  types.rs
  layout.rs
  profile.rs
  lba0.rs
  lba1.rs
  ...
  lba12.rs

tests/
  protocol_layout.rs
  protocol_profiles.rs
  protocol_byte_coverage.rs
  protocol_lba0.rs
  ...
  protocol_lba12.rs
  fixtures/protocol_evidence/

audit/protocol/
  field_catalog.tsv
  evidence_manifest.tsv
  profile_coverage.tsv
  artifacts/

docs/
  EDP_LBA0_12_FIELD_GUIDE.md
  EDP_PROTOCOL_REVERSE_ENGINEERING.md
  EDP_PROTOCOL_ENGINEERING_PLAN.md
```

现有逆向总文档继续作为 provenance / 推导记录，不再承担日常协议 API 手册职责。

---

## 4. Field Catalog：统一机器语义源

新增 `audit/protocol/field_catalog.tsv`。

每一行至少包含：

```text
field_id
lba
start
end
length
profile_axis
profile
semantic_type
meaning
decode_rule
encode_rule
ownership
evolution_kind
producer_evidence
consumer_evidence
physical_evidence
implementation_provenance
code_symbol
test_symbol
semantic_status
implementation_status
behavior_test_status
ownership_test_symbol
```

要求：

1. LBA0–LBA12 的 6656 个物理字节必须全部覆盖；
2. 同一个 `LBA + offset + profile` 恰好只有一个 owner；
3. profile 分叉必须显式展开；
4. preserve / opaque / backing 也必须成为正式 semantic type，不能留空；
5. 字段不得仅以“reserved”命名而没有 ownership 行为；
6. `semantic_status=COMPLETE` 只表示语义闭环，允许同时为 `implementation_status=PLANNED`、`behavior_test_status=PLANNED`，不降低 6656B 语义覆盖。
7. `implementation_status=COMPLETE` 必须链接到真实且经编译期登记的 `code_symbol`；`behavior_test_status=COMPLETE` 必须链接到真实且经编译期登记的 `test_symbol`。两个工程状态分别升级，允许值为 `PLANNED | COMPLETE`。
8. PLANNED 链接只能是 `planned:...` 设计目标或 `UNIMPLEMENTED`，不能算作正式符号。通用 ownership 测试单独记录为 `ownership_test_symbol`，不能充当字段行为测试。
9. Phase 3/4 升级时，在 `tests/protocol_field_catalog.rs` 的符号登记表加入可编译 Rust 路径，再同步更新对应行状态和链接；行为测试放在该 integration test 或其导入的测试模块。登记证明符号存在，review 仍须核对字段关联与行为断言，不能以存在性替代行为正确性。

推荐 semantic type：

```rust
pub enum SemanticType {
    Scalar,
    CString,
    PackedStruct,
    EncryptedRegion,
    Checksum,
    ProfileSelector,
    OpaquePreserve,
    UnownedBacking,
    CompatibilityField,
    Snapshot,
}
```

---

## 5. 人类可读字段手册

新增：

`docs/EDP_LBA0_12_FIELD_GUIDE.md`

不按逆向时间线写，严格按 LBA0 → LBA12。

每个 LBA 固定结构：

### A. 扇区用途概览

解释这个扇区解决什么问题，以及哪些 profile 会使用它。

### B. 512B 布局图

例如：

```text
0x000 ┌─────────────────────────────┐
      │ Dept / SAFE6 header         │
0x040 ├─────────────────────────────┤
      │ SAFE6 body                  │
...
0x1E0 ├─────────────────────────────┤
      │ legacy MBR entry snapshot   │
0x1EE ├─────────────────────────────┤
      │ metadata/checksum tail      │
0x200 └─────────────────────────────┘
```

### C. 字段表

固定列：

| Offset | Length | Field ID | Profiles | Type | Meaning | Code |
|---|---:|---|---|---|---|---|

### D. 逐字段解释

每个字段解释：

- 原始字节范围；
- 字节序；
- 编码/解码；
- producer；
- consumer；
- profile 分叉；
- preserve/backing 规则；
- 与其它 LBA 的关系；
- implementation provenance 边界。

### E. 历史演进表

对存在历史差异的扇区增加 profile matrix，例如：

| Region | short | join59 | join60/current |
|---|---|---|---|

### F. 测试入口

每个 LBA 末尾列出：

- parser；
- profile detector；
- fixture；
- 对应 Rust tests。

---

## 6. 将协议设计变成 Rust 代码

新增 `src/protocol/`，不再让协议只存在于 audit 脚本和测试里的 magic offset。

### 6.1 每个 LBA 一个明确 parser

目标接口：

```rust
pub fn parse_lba0(raw: &[u8; 512]) -> Result<Lba0View, ProtocolError>;
pub fn parse_lba1(raw: &[u8; 512]) -> Result<Lba1View, ProtocolError>;
// ...
pub fn parse_lba12(raw: &[u8; 512]) -> Result<Lba12View, ProtocolError>;
```

并提供整镜像入口：

```rust
pub fn parse_protocol_image(
    raw: &[u8; 6656],
) -> Result<ProtocolImage, ProtocolError>;
```

### 6.2 profile-dependent 字段必须用 enum

例如：

```rust
pub enum Lba6Byte63 {
    ShortBacking(u8),
    Join59Terminator,
    Join60DeptByte59(u8),
}
```

而不是错误地永久定义成：

```rust
dept_byte_59: u8
```

### 6.3 opaque/preserve 也必须有正式类型

例如 LBA3：

```rust
pub struct Lba3OpaquePreserve {
    pub raw: [u8; 512],
}
```

这表示“EDP 生命周期语义已经闭环为 preserve-only”，不是“内部 512B 厂商字段均已解码”。

### 6.4 跨 LBA 关系写成代码约束

例如 legacy MBR snapshot 与 LBA12 type4：

```text
LBA6 snapshot start_lba == LBA12 type4 StartSector
LBA6 snapshot sector_count == LBA12 type4 PartionSize / 512
```

不能只留在文档里。

---

## 7. 逐字节、逐 profile 测试

新增 `tests/protocol_byte_coverage.rs`。

### 7.1 6656B ownership gate

逐个扫描：

```text
LBA0 offset 0x000
...
LBA12 offset 0x1FF
```

要求：

- 每个字节至少有一个已知 profile definition；
- 每个 profile 下恰好一个 owner；
- 不允许 undocumented gap；
- 不允许 ownership overlap；
- Field ID 必须可追到 parser 和测试。

最终输出必须能回答：

```text
LBA6 +0x1E4
-> field: lba6.legacy_mbr_snapshot.start_lba[0]
-> profile: legacy-mbr-snapshot
-> semantic: Snapshot
-> test: legacy_mbr_snapshot_matches_lba12_type4
```

### 7.2 不是只测 6656 个位置，而是测 profile-state coverage

最终门禁至少统计：

```text
Physical byte ownership: 6656 / 6656
Known profile-state definitions: 100%
Undefined profile mappings: 0
Overlapping ownership mappings: 0
```

### 7.3 四层测试

1. **layout tests**
   - offset；
   - length；
   - overlap；
   - full coverage。

2. **decode tests**
   - physical fixture → typed Rust structure。

3. **encode/reconstruction tests**
   - 对有 writer 规则的字段验证反向输出。

4. **profile tests**
   - current；
   - legacy；
   - join59；
   - join60；
   - GPT enabled/absent；
   - LBA12 mode1/2/3/legacy；
   - EESI；
   - LBA11 DiskSize / CHS；
   - 其它已知 profile。

---

## 8. 完全移除对外部 u_disk 的运行时依赖

最终要求：

```bash
git clone ...
cargo test
```

即可完成 LBA0–LBA12 协议验证。

不得要求：

```text
/Users/zhangyuxi/Desktop/u_disk
~/Desktop/u_disk
/private/tmp/<historical DLL>
```

### 8.1 fixture 迁移原则

证据按“最小充分”迁入当前仓库：

- 能用 512B sector fixture 证明的，不复制完整镜像；
- 需要跨 LBA 关系时保留最小 6656B image；
- 需要验证机器码时，优先保存：
  - 固定代码片段；
  - hash；
  - VA；
  - extraction metadata；
- 只有确实必须执行原二进制才能证明的实验，才考虑保存对应 binary artifact，并明确 license/provenance 和 hash。

### 8.2 外部路径门禁

新增测试：

`protocol_has_no_external_runtime_fixture_dependency`

扫描：

- `tests/`
- `src/protocol/`
- `scripts/protocol/`

禁止运行时代码读取：

- `/Users/zhangyuxi/Desktop/u_disk`
- `~/Desktop/u_disk`
- `/private/tmp`

历史文档中可以作为 provenance 文本出现，但不能成为测试运行依赖。

---

## 9. 现有 audit 脚本的处理

按三类整理：

### A. 协议规则已经稳定

迁移为正式 Rust tests，Python 只保留必要的独立交叉验证。

### B. 二进制 provenance 仍有长期价值

继续保留在 `scripts/protocol/`，但改成读取仓库内 artifact/fixture。

### C. 已被后续证据完全取代

从日常门禁移除；必要时保留到历史归档，不再让新 AI 把旧阶段结论当现状。

目标是：

**日常协议验收不需要理解几十个逆向阶段脚本。**

---

## 10. 自动生成协议覆盖报告

新增机器输出，例如：

```text
target/protocol-coverage.json
target/protocol-coverage.md
```

报告至少包含：

- 13个 LBA；
- Field ID；
- offsets；
- length；
- profile states；
- semantic type；
- parser symbol；
- test symbol；
- fixture；
- evidence IDs；
- implementation provenance 状态。

最终摘要：

```text
LBA0-LBA12 bytes mapped: 6656 / 6656
Known profile states mapped: 100%
Fields without parser: 0
Fields without tests: 0
Ownership overlaps: 0
Undocumented bytes: 0
External runtime fixture dependencies: 0
```

---

## 11. 分阶段执行顺序

### Phase 1：统一字段模型

1. 创建 `field_catalog.tsv`；
2. 从当前 canonical byte ledger 拆出 field × profile；
3. 加入完整 6656B ownership 检查；
4. 不改变现有业务代码行为。

**退出条件：**
机器目录可完整描述所有 LBA0–12 字节和已知 profile。

### Phase 2：人类字段手册

1. 创建 `EDP_LBA0_12_FIELD_GUIDE.md`；
2. 严格按 LBA0–12；
3. 每个字段有 offset / meaning / profile / code / test；
4. 添加历史演进 matrix。

**退出条件：**
不读逆向时间线也能完整理解盘面布局。

### Phase 3：Rust 协议模型

1. 创建 `src/protocol/`；
2. 实现 profile types；
3. 实现 LBA0–12 parser；
4. 实现整镜像 parser；
5. 把跨 LBA invariant 写进 typed validation。

**退出条件：**
协议不再依赖 magic offsets 才能理解。

### Phase 4：逐字节/逐 profile 测试

1. layout；
2. byte ownership；
3. decode；
4. profile；
5. reconstruction；
6. cross-LBA。

**退出条件：**
6656B × 已知 profile state 全部有机器测试。

### Phase 5：证据内聚

1. 清点所有 `u_disk` / `/private/tmp` 依赖；
2. 提取最小 fixture/artifact；
3. 修正 audit 脚本路径；
4. 增加外部路径零依赖门禁。

**退出条件：**
clean clone 不访问用户个人目录。

### Phase 6：收口与精简

1. 删除/归档被取代的旧审计入口；
2. 自动生成 coverage report；
3. 文档 ↔ catalog ↔ Rust symbols ↔ tests 一致性测试；
4. README/AGENTS 增加协议开发入口说明。

---

## 12. 最终验收标准

最终必须同时满足：

```text
LBA0-LBA12 semantic coverage       = 6656 / 6656
Byte ownership coverage            = 6656 / 6656
Known profile-state coverage       = 100%
Fields without typed parser        = 0
Fields without behavior test       = 0
Undocumented byte gaps             = 0
Ownership overlaps                 = 0
External u_disk runtime dependency = 0
External /private/tmp dependency   = 0
Documentation/schema drift         = 0
cargo fmt                          = PASS
cargo test                         = PASS
clean-clone protocol tests         = PASS
```

## 13. 给后续 AI 的阅读顺序

完成工程化后，后续 AI 应按顺序阅读：

1. `docs/EDP_LBA0_12_FIELD_GUIDE.md` —— 人类协议入口；
2. `src/protocol/layout.rs` / `profile.rs` —— 机器协议入口；
3. `audit/protocol/field_catalog.tsv` —— 字段/profile 全表；
4. `tests/protocol_byte_coverage.rs` —— 6656B 完整性门禁；
5. `docs/EDP_PROTOCOL_REVERSE_ENGINEERING.md` —— 仅在需要 provenance / 历史推导时阅读。

逆向历史文档不应再成为理解协议的必读入口。


---

## 14. 官方制盘复刻与“已有官方盘 → 二合一”设计

### 14.1 总原则：两个正交轴，不再用一个 canonical profile 混在一起

正式制盘必须至少把以下两个轴分开：

#### 轴 A：官方分区布局模式

已由官方 PDF、LabelTool UI、request structure、`CreatePartitions` 和真实盘闭环：

```text
part=0  DefaultThreePartition
        默认三分区
        type1 Boot + type2 Share + type4 Encrypt

part=1  BootExchangeMerged
        启动区与交换区二合一
        type2 Share + type4 Encrypt

part=2  AllEncrypt
        整盘加密
        官方两-entry 特殊布局

part=3  InOutNetDual
        内外网通用双分区
        type1 Boot + type2 Share
```

实现中必须建立正式 enum，例如：

```rust
pub enum OfficialPartitionMode {
    DefaultThreePartition,
    BootExchangeMerged,
    AllEncrypt,
    InOutNetDual,
}
```

#### 轴 B：LBA12 wrapped-file-key 算法

与上面的 `part` 布局独立：

```text
legacy mode0 compatibility
mode1  A6B0/A7F0 legacy-family wrapping
mode2  SM4-ECB wrapped file key
mode3  AES-128-ECB wrapped file key
```

注意：

> LBA12 整个 sector 的外层保护仍是 device-id CRC 派生的 A6B0/A7F0；
> mode1/2/3 描述的是 `NeedEncrypt != 0` EDPF entry 内的 wrapped file-key algorithm，
> 不是“整扇 LBA12 分别使用三种算法”。

正式 spec 应类似：

```rust
pub struct OfficialProvisionSpec {
    pub partition_mode: OfficialPartitionMode,
    pub wrapped_key_mode: Lba12Mode,
    pub geometry: OfficialGeometry,
    pub identity: TargetIdentity,
    pub metadata: ProvisionMetadata,
    pub password_policy: PasswordPolicy,
}
```

禁止再用一个 `canonical_v1` 同时暗含分区模式、密钥算法、历史表示和产品策略。

### 14.2 新盘制盘：复刻官方 producer，而不是复刻某一只 donor 盘

“从普通 U 盘新制盘”的目标是 first-party producer-compatible，而不是 donor cloning。

建议流水线：

```text
probe hardware identity
    ↓
select OfficialPartitionMode
    ↓
select LBA12 wrapped-key mode
    ↓
calculate official geometry
    ↓
generate file keys / password wrapping
    ↓
build typed protocol image LBA0-LBA12
    ↓
ProtocolImageView cross-LBA validation
    ↓
write metadata
    ↓
create/format required data partitions
    ↓
deploy mode-specific files/resources when applicable
    ↓
read back metadata + filesystem geometry
    ↓
final validation
```

各官方模式的 writer 必须由同一组 typed primitives 组合，不能复制四套 magic-offset 代码。

### 14.3 官方 BootExchangeMerged 的已闭环目标形态

当前官方 `part=1` 已闭环为：

```text
EDPF:
entry0 = type2
entry1 = type4
PartionCount = 2

LBA0 MBR:
single visible partition
type  = 0x07
start = 63
count = type2 sector count

LBA12:
type2 starts at 63
type4 starts immediately after type2
```

因此“二合一”不是简单扩大一个独立 type1 Boot。

current producer 的精确语义是：

- 独立 type1 被取消；
- 前部大区成为 type2；
- 标准 MBR 直接暴露同一 type2 物理范围；
- OS 因此无需先走 EDP UserLogin 即可访问前部文件系统；
- type4 仍由 EDP 安全挂载链管理。

### 14.4 特殊模式：已有官方盘转换为二合一

这一模式必须和“新盘 provision”使用不同 API，建议命名：

```rust
convert_official_to_merged(...)
```

而不是复用：

```rust
generate_image(...)
```

核心原则：

> **Preserve → Transform，禁止 Rebuild-from-template。**

首先只读解析现有官方盘，保存其真实协议状态：

- LBA0–LBA12 原始 6656B；
- device_id / VID / PID / total size；
- onlyid；
- LBA3 opaque manufacturer data；
- LBA4 representation / overlays；
- LBA6 SAFE6 backing、Dept/User、MBR snapshot profile；
- LBA8 ELABEL / HostHardinfo / backing；
- LBA7 / LBA12 EDPF；
- PassInfo；
- 原 UserKeyCRC / FileKeyCRC / wrapped file-key / EncryptMode；
- LBA11 DRKB/PDKB 与随机 payload；
- type2/type4 真实 StartSector / PartionSize；
- 当前文件系统 geometry。

### 14.5 转换不允许改变 type4，除非显式进入未来的“数据迁移模式”

第一版转换器的非协商约束：

```text
old type4 StartSector == new type4 StartSector
old type4 PartionSize == new type4 PartionSize
old wrapped file key  == new wrapped file key
old FileKeyCRC        == new FileKeyCRC
old UserKeyCRC        == new UserKeyCRC
```

即：

- 不移动保密区；
- 不重新加密保密区；
- 不生成新的 type4 file key；
- 不改变保密区密文；
- 不把“改成二合一”实现成全盘解密/重写。

新的 type2 geometry 固定由原 type4 起点反推：

```text
new type2 StartSector = 63
new type2 SectorCount = old type4 StartSector - 63
```

这样保密区可以物理原地保留。

### 14.6 为什么转换不能只写 LBA0-LBA12

普通三分区盘前部通常存在：

```text
type1 Boot
type2 Share
type4 Encrypt
```

转换后要求：

```text
type2 from LBA63
type4 Encrypt unchanged
```

即使 metadata 可以改成上述目标，原 type1 + type2 两个前部文件系统并不会自动变成一个合法的大 type2 文件系统。

因此第一版转换器必须明确区分：

#### A. Metadata-only 可行

只有当现有前部物理文件系统已经满足目标 geometry，或经过严格证明无需移动/重建即可扩展时，才允许 metadata-only。

必须先通过 filesystem geometry validator，不能仅凭 EDPF 推断。

#### B. Front-region rebuild

默认、安全、可证明的转换路径：

```text
backup front-region user files（可选）
    ↓
freeze old type4 boundary
    ↓
recreate one filesystem on LBA63 .. old_type4_start-1
    ↓
restore front-region files（可选）
    ↓
rewrite minimal EDP metadata
    ↓
readback validate
```

这里允许重建**前部非保密区**，但仍不得移动或改写 type4 数据区。

第一版不实现未经证明的 in-place filesystem move/grow 魔法。

### 14.7 转换时允许修改的协议范围

最小变更集合应从 typed diff 自动推导，而不是硬编码“写 0/6/7/12”。

预期主要包含：

- LBA0：标准 MBR 改为单 `0x07 @ LBA63`；
- LBA7：entry count / partition type / geometry；
- LBA12：current EDPF entry count / partition type / geometry；
- LBA6：仅当 legacy MBR snapshot profile 需要同步 type4 geometry 时更新 snapshot/checksum；
- 其它扇区默认 preserve。

明确默认 preserve：

- LBA3；
- LBA4 onlyid / HSerial / representation；
- LBA8 ELABEL / identity / opaque backing；
- LBA10 EESI；
- LBA11 DRKB/PDKB；
- type4 key material；
- 未被转换规则拥有的所有 backing/opaque bytes。

若实际 typed diff 出现额外字段变化，转换必须 fail-closed 并要求 review。

### 14.8 安全执行事务

转换命令必须先产生只读计划，建议：

```text
edpcli convert-official-to-merged <device> --plan
```

计划至少输出：

- 当前 profile；
- 目标 profile；
- old/new EDPF；
- old/new MBR；
- type4 immutable boundary；
- 需要重建的前部范围；
- 将改写的 sector/byte range；
- 将 preserve 的 sector/range；
- 文件系统处理方式；
- rollback artifact；
- 最终 validator 列表。

真正执行前必须：

1. 重做硬件身份检查；
2. 备份原 LBA0–LBA12；
3. 备份所有将修改的前部 boot/partition metadata；
4. 再次确认 type4 起点、大小、hash/sample 未变化；
5. whole-unmount；
6. 前部文件系统操作；
7. metadata rewrite；
8. readback；
9. `ProtocolImageView` validation；
10. 原生 OS mount 验证 type2；
11. EDP 安全链验证 type4 可识别；
12. 任一步失败立即停止后续写入并提供回滚。

### 14.9 测试策略

新制盘与转换测试分开。

#### Fresh official provisioning tests

必须覆盖：

- 四种 `OfficialPartitionMode`；
- mode1 / mode2 / mode3；
- legacy compatibility 仅按实际有证据的 producer 规则；
- LBA0–LBA12 typed parse；
- cross-LBA invariant；
- MBR/EDPF geometry；
- filesystem visible/hidden behavior。

#### Existing-official → merged tests

第一阶段只允许虚拟镜像/软件块设备：

- 默认三分区 → 二合一；
- 不同 Dept layout；
- LBA6 zero-underlay / legacy snapshot；
- current/legacy LBA4 representation；
- type4 起点/大小保持不变；
- type4 区内容 hash 前后不变；
- front rebuild 后 MBR/type2 filesystem 可挂载；
- rollback 可恢复原 metadata。

真实盘写入必须继续沿用当前 fail-closed 设备身份和 readback 机制，且不得为了测试访问真实 raw disk。

### 14.10 实现顺序

正式制盘重构按以下顺序：

1. 完成当前 `ProtocolImageView` 与 cross-LBA validator；
2. 把现有 `ProvisionProfile::canonical_v1()` 拆成 orthogonal provision spec；
3. 实现官方四种 partition mode 的纯内存 builder；
4. 用 first-party/physical fixtures 做 byte-level differential tests；
5. 实现 fresh-provision filesystem stage；
6. 新增 `ConversionPlan`，只读分析已有官方盘；
7. 实现 front-region rebuild + metadata minimal transform；
8. 实现 readback / rollback；
9. 虚拟盘全矩阵；
10. 最后才允许真实盘显式执行。

在第 1–4 步完成前，不继续扩展旧 canonical writer，以免形成第二套错误协议事实源。
