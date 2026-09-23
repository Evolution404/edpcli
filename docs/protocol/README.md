# EDP 协议文档

## 当前事实源

1. `audit/protocol/byte_ledger.tsv`、field catalog、profile coverage 和对应测试：机器可验证事实源。
2. [`EDP_LBA0_12_FIELD_GUIDE.md`](EDP_LBA0_12_FIELD_GUIDE.md)：LBA0–12 日常阅读入口。
3. [`EDP_PROTOCOL_REVERSE_ENGINEERING.md`](EDP_PROTOCOL_REVERSE_ENGINEERING.md)：唯一协议逆向总文档，用于 provenance、历史 profile、证据推导。
4. [`LCE.md`](LCE.md)：LCE 专题。

当前 canonical LBA0–12 语义覆盖为 **6656/6656B COMPLETE**。`COMPLETE` 表示 EDP 协议边界的 producer/caller-owned serialization/consumer 生命周期已闭环；它不等同于所有 profile 都存在真实物理正例，也不要求把制造商私有 opaque payload 的内部格式继续拆解。

物理覆盖缺口单独记录在 `audit/protocol/profile_coverage.tsv`，不得降格或升级 byte-semantic 状态。

## LCE 简称

**LCE = LBA7 Compatibility Extent**。第一次出现可写全称，后续统一使用 LCE。旧称“Region A”和“legacy type4 extent”均已废弃。

## 分析笔记

仍有长期价值但不应作为当前状态表的专项调查放在 `audit/protocol/notes/`。这些笔记只能补充 provenance/边界，不能覆盖 ledger 与 canonical 文档。
