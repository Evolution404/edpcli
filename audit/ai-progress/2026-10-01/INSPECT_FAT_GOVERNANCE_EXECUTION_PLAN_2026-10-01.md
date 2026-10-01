# Inspect + FAT 能力统一实施总纲（2026-10-01）

## 1. 文档关系

专项计划：
- `audit/ai-progress/2026-10-01/INSPECT_GOVERNANCE_2026-10-01.md`
- `audit/ai-progress/2026-10-01/FAT_CAPABILITY_AND_PROVISION_FORMAT_PLAN_2026-10-01.md`

本文件只负责阶段依赖、提交边界和最终门禁。

## 2. 工作区

规划 worktree：`/Users/zhangyuxi/.webcodex-managed-worktrees/edpcli-3ca4ed72`
规划分支：`plan/inspect-governance-20261001`
建立时基线：`main / origin/main = cb57c2a44b4eb658fea0fc0f6aaa79440e6abb76`

主工作区存在并行 Provision 修改时，本 worktree 不触碰、不覆盖、不 reset/clean。

## 3. 总体原则

1. 先修 Inspect Loading 生命周期和信息架构，再改 Mixed 数据模型。
2. Decode provenance 必须由 protocol decode 层提供，TUI 不推测。
3. Mixed 的 FieldStatus 与 DecodeStatus 正交。
4. 删除旧 Tab 时同时删除死状态/死 renderer，避免只隐藏 UI 留技术债。
5. FAT32 完整 driver 完成前，不开放 TUI FAT32。
6. FAT16 OEM Name 改动必须基于官方/真实样本证据。
7. 不新增 FilesystemKind。
8. 每个阶段单独测试、单独提交。
9. 不以手工观察替代自动测试。
10. 真实 USB/HIL 放在纯逻辑、虚拟盘、TUI tests 全绿之后。

## 4. 推荐实施顺序

### Stage A — I0 Loading 生命周期
目标：Disk/Backup Inspect 都改成来源页居中 modal；成功才进入 Inspect；失败留来源页；共用 spinner；删除全屏 Running page。
建议提交：`refactor(inspect): move loading into source-page modal`。
门禁：loading state、Disk/Backup navigation、modal center、无 Browser+None。

### Stage B — I1/I2 Browser 架构债治理
目标：删除顶部“业务字段 / 原始字段 / Hex”；统一 Browser；保留左侧结构树；右上改对象摘要；底部改字段/证据；落实节点内容矩阵。
建议提交：
- `refactor(inspect): collapse legacy browser tabs`
- `refactor(inspect): separate object summary from field evidence`

### Stage C — I3 Decode provenance
目标：protocol adapter/parser 显式提供 decoded ranges 与 availability/error；AdvancedInspectItem/workspace/cache 携带 provenance；TUI 不做 raw!=decoded 推断。
建议提交：`feat(inspect): carry decode provenance with sector evidence`。
门禁：00→00+Decoded、partial ranges、Unavailable、cache roundtrip、Disk/Backup parity。

### Stage D — I4/I5 Mixed 与扇区检查
目标：FieldStatus fg；DecodeStatus independent overlay；Cursor/Search overlay；Unknown/Reserved 视觉拆分；Hex/ASCII 同步；字节/字段详情展示 decode provenance。
建议提交：
- `fix(inspect): render mixed mode from decode provenance`
- `refactor(inspect): compose field decode and interaction styles`

### Stage E — I6 Inspect 收口
目标：J 全局跳转；lazy read/cache；decode error；modal layering；删除旧 Business/RawFields/Hex 代码；删除重复 evidence 文本；文案统一。
建议提交：`chore(inspect): close navigation and legacy cleanup`。

### Stage F — F1 FAT32 完整 driver
目标：matches_geometry、metadata、format、verify、driver tests。
建议提交：`feat(filesystem): complete fat32 format and verify driver`。

### Stage G — F2 FAT analyzer
目标：FAT32 analyze、FAT12 analyze、复用 shared FAT parser、capabilities 更新。
建议提交：`feat(filesystem): extend shared fat analysis to fat12 and fat32`。

### Stage H — F3 FAT16 OEM
目标：取证官方 FAT16 OEM field；移除 formatter 中 EDPCLI；fixture/regression。
建议提交：`fix(filesystem): align fat16 oem field with official media`。
特别门禁：真实写盘 formatter 不得再写 EDPCLI。

### Stage I — P1 Provision FS Picker
目标：单一 FAT16/FAT32/exFAT 支持列表；EDP/Plain 共用；forward/reverse；默认值不变；capability gate。
建议提交：`feat(provision): offer fat16 fat32 and exfat formatting`。

### Stage J — P2 全链路验收
执行顺序：formatter/unit → filesystem contract → provision layout → application provision → TUI state/render → full gate → Virtual Disk → 必要真实 USB/HIL。
禁止跳过前置测试直接上真实盘。

## 5. Mixed semantic 测试矩阵

| Field | Decode | Interaction | Raw→Decoded | 期望 |
|---|---|---|---|---|
| Known | Decoded | Normal | 00→00 | Known fg + Decode overlay |
| Unknown | Decoded | Normal | 00→00 | Unknown fg + Decode overlay |
| Reserved | Decoded | Normal | A5→00 | Reserved fg + Decode overlay |
| Preserved | Decoded | Normal | 11→11 | Preserved fg + Decode overlay |
| None | Decoded | Normal | 22→22 | Normal fg + Decode overlay |
| Known | Plain | Normal | A5→A5 | Known fg only |
| Known | Unavailable | Normal | — | Known fg + Warning overlay |
| Known | Decoded | Cursor | 00→00 | semantic layers + cursor |
| Unknown | Decoded | SearchMatch | 00→00 | semantic layers + search |

硬门禁：`Raw == Decoded` 不得改变 DecodeStatus。

## 6. Browser 验收矩阵

整盘：摘要 capacity/disk kind/protocol/region count；证据 region table。
Protocol：摘要 LBA0–12 completeness/diagnostics/decode availability；证据 LBA list。
LBA：摘要 role/512B/key conclusions/validation；证据 Field table。
Partition：摘要 start/end/size/filesystem/encrypted/plain；证据 geometry/boot/filesystem facts。
Field：摘要 name/value/status；证据 Raw/Decoded/ranges/source。
Unknown/Reserved/Preserved：只展示可证明范围、状态、Raw/Hash，不推测。

## 7. Provision FS 验收矩阵

支持列表固定：FAT16 / FAT32 / exFAT。
默认：Boot FAT16；Share exFAT；Encrypt exFAT。

| FS | EDP Picker | Plain Picker | Format | Verify |
|---|---:|---:|---:|---:|
| FAT16 | ✓ | ✓ | ✓ | ✓ |
| FAT32 | ✓ | ✓ | ✓ | ✓ |
| exFAT | ✓ | ✓ | ✓ | ✓ |
| FAT12 | × | × | × | × |
| NTFS | × | × | × | × |

## 8. 最终技术债清理检查

Inspect：
- 无全屏 Loading。
- 无用户侧 Business/RawFields/Hex tabs。
- 无 equality-based Mixed coloring。
- 无 FieldStatus/DecodeStatus 竞争式 else-if。
- 无 Hex/ASCII 语义分裂。
- 无 Browser/result=None。
- 无重复 summary/evidence 文本。
- Disk/Backup 功能一致。

Filesystem：
- FAT32 不再是“可识别但不可格式化”的半成品。
- FAT12 analyzer 可用。
- FAT16 formatter 不写 EDPCLI。
- Provision picker 不复制多套 supported-FS match。
- NTFS 不误出现在制盘 picker。

## 9. 最终完成定义

只有以下全部通过才关闭本计划：
- Inspect I0–I6。
- FAT F1–F3。
- Provision P1–P2。
- fast gate。
- full gate。
- Virtual Disk。
- 必要 USB/HIL。
- 文档更新。
- legacy grep 清理。
- 所有阶段形成清晰、可回滚提交历史。
