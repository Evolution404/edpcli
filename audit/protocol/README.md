# 协议审计可复现基线

本目录是 `docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md` 的机器可读伴随证据层，不替代标准协议文档。它用于使文档背后的证据可复现，并拒绝静默的字节账本漂移。

## 证据类型

- `physical`：从允许使用的真实设备金标来源采集的字节。
- `virtual`：在隔离的文件/内存后端环境中执行官方二进制得到的输出或行为。虚拟证据绝不能改标成物理采集。
- `static`：PE/ELF/DWARF/反汇编证据，例如精确写入、读取、分支、函数地址或 ABI 字段边界。

`evidence_manifest.tsv` 记录具体产物和锚点。`gold_samples.tsv` 按来源名、仓库路径、长度和 SHA-256 冻结当前 **通用统计集** 样本集。完整 LBA0-LBA12 统计集字节已经提交到 `audit/protocol/gold/`，因此无需访问采集工作站即可从干净克隆复现该样本集。主机本地的可执行二进制仍只记录摘要，不复制进仓库。

某些协议配置类型需要特定用途物理正例来闭环，但不应改变通用统计集。这类证据放在 `audit/protocol/physical-evidence/`，必须进入清单、按摘要固定并由定向回归测试覆盖；除非通用统计集政策本身发生变化，否则不得并入 `gold_samples.tsv`。

基线脚本对样本集漂移采用无法确认即拒绝继续：新的非 `_nopwd_` 备份不会被静默纳入或忽略，必须先人工审计，并把来源及摘要显式加入 `gold_samples.tsv`。

## 金标数据与来源

通用统计集的协议金标字节集位于已经提交的 `audit/protocol/gold/`：

1. `audit/protocol/gold/strict-encrypted/`：恰好 19 份 SHA-256 唯一的 6656 字节原始代真实设备镜像。
2. `audit/protocol/gold/authentic-nopwd/`：1 份 6656 字节真实 SanDisk Ultra 免密盘采集，仅包含 LBA0-LBA12。

另有一份 EESI 启用的 Netac 正向采集保存在 `audit/protocol/physical-evidence/eesi/`。它是字段级物理参考，不是第 21 个统计集成员；这样既保留 19+1 的跨配置类型统计口径，又允许 LBA10 的启用配置类型满足真实设备证据门禁。

清单中每一行都必须具有唯一 SHA-256。完整 6656 字节完全相同的重复只读采集必须去重，禁止重新作为独立金标样本加入。

原始采集来源仍记录为：加密样本的 `~/.edpcli-backup`，以及 SanDisk 样本的 `~/Desktop/u_disk/analyze/disk_data/no_password_disk4/raw/LBA0_13_concat.bin`。这些主机本地路径只保留来源信息，不再是 `audit_baseline.py` 的运行依赖。原始 SanDisk 文件还包含 LBA13，但仓库中的金标镜像有意截断为 6656 字节。

旧的 `/private/tmp/audit22` 测试框架曾把第三份 SanDisk EESI 采集混入样本集。其源码/可执行文件摘要仍在清单中保留作为来源记录，但对应样本集定义已经废弃。当前 `scripts/protocol/audit_baseline.py` 在仓库内按新的两类来源政策复现有价值的统计集行为，不再依赖 `/private/tmp`。

## 字节账本

`byte_ledger.tsv` 对 LBA0-LBA12 的每个字节恰好分区一次。每一行记录状态、适用配置类型，以及相互独立的写入端、消费端和物理证据 ID。`tests/protocol_byte_ledger.rs` 会展开所有范围表达式，并拒绝重叠、缺口、非法证据引用以及与标准严格进度表不一致的情况。

需要查看人类可读的逐字节内容时运行：

```text
python3 scripts/protocol/query_byte_ledger.py --lba 4
python3 scripts/protocol/query_byte_ledger.py --lba 3 --offset 0x20
python3 scripts/protocol/query_byte_ledger.py --image <6656-byte-image> --lba 10
```

带镜像参数的形式会在账本状态、字段/区域、配置类型和证据 ID 旁显示物理偏移及字节。与解密有关的偏移，在对应解码器明确注册进查询工具之前仍以标准字段说明为准；查询工具不得臆造解密视图。

## 重新运行基线检查

```text
python3 scripts/protocol/audit_baseline.py
cargo test --test protocol_byte_ledger
cargo test --test protocol_documentation_contract
```

基线审计全程只读，不打开原始磁盘设备，也不写入任何金标采集。

## 分析笔记

`notes/` 保存仍具有长期证据价值的来源/边界调查。它们不是当前状态账本，可能包含历史假设或已经被后续结论替代的阶段性进度。当前语义状态始终以 `byte_ledger.tsv`、`field_catalog.tsv`、`profile_coverage.tsv` 及其测试为准。

## 本地可执行文件完整性

`audit/protocol/notes/labeltool_variant_diff.md` 记录三份本地 `cemssafeudisklabeltool*.exe` 的逐字节比较。只有 `cemssafeudisklabeltool_orig.exe` 被视为官方前端基线；另外两份包含本地施加的策略/校验绕过，禁止作为官方写入端证据。
