# EDP LBA0–LBA12 协议工程化实施计划

> 状态基线：LBA0–LBA12 共 6656B 的 EDP 字节语义已完成 6656/6656B = 100.0% 闭环。
>
> 本计划的目标不是继续堆叠逆向笔记，而是把既有结论工程化为：
> 1. 人类可直接阅读的字段手册；
> 2. 可被 Rust 代码直接消费的协议模型；
> 3. 覆盖 LBA0–LBA12 每个字节、每个已知 profile 的结构化测试；
> 4. 完全纳入 edpcli 仓库、clean clone 可重放且不依赖外部 `u_disk` 目录的证据与测试体系。

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
```

要求：

1. LBA0–LBA12 的 6656 个物理字节必须全部覆盖；
2. 同一个 `LBA + offset + profile` 恰好只有一个 owner；
3. profile 分叉必须显式展开；
4. preserve / opaque / backing 也必须成为正式 semantic type，不能留空；
5. 字段不得仅以“reserved”命名而没有 ownership 行为；
6. 所有 COMPLETE 字段必须链接到正式代码 symbol 和测试 symbol。

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
