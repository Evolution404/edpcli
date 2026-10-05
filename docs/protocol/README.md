# EDP 协议文档

## 当前事实源

1. `audit/protocol/byte_ledger.tsv`、`field_catalog.tsv`、`profile_axes.tsv`、配置类型覆盖和对应测试：机器可验证事实源。`profile_axes.tsv` 同时登记 18 个正交轴的检测器符号与检测模式。
2. [`EDP_LBA0_12_FIELD_GUIDE.md`](EDP_LBA0_12_FIELD_GUIDE.md)：LBA0–12 日常阅读入口。
3. [`EDP_PROTOCOL_REVERSE_ENGINEERING.md`](EDP_PROTOCOL_REVERSE_ENGINEERING.md)：唯一协议逆向总文档，用于来源记录、历史配置类型和证据推导。
4. [`LCE.md`](LCE.md)：LCE 专题。

当前标准 LBA0–12 语义覆盖为 **6656/6656B COMPLETE**。`COMPLETE` 表示 EDP 协议边界的写入端/调用方负责的序列化/消费端生命周期已闭环；它不等同于所有配置类型都存在真实物理正例，也不要求把制造商私有不透明负载的内部格式继续拆解。

物理覆盖缺口单独记录在 `audit/protocol/profile_coverage.tsv`，不得降格或升级字节语义状态。

## 配置类型检测器目录

运行时统一入口为 `edpcli::protocol::profile_detector::PROFILE_AXIS_DETECTORS` 与 `detect_profile_axes()`。检测结果分为 `Unique`、`Candidates`、`MissingContext`、`Unknown`；只有盘面与所需上下文能够唯一证明时才返回 `Unique`。`lba4_encoding`、`lba4_second_key_source`、`lba4_hserial_source`、`host_hardinfo_source` 属于 `provenance-only` 轴，禁止根据字节形状反推写入端来源。字段指南由 `profile_axes.tsv` 自动展示检测器与检测模式，测试会校验 TSV 与 Rust 运行时目录完全一致。

## LCE 简称

**LCE = LBA7 兼容扩展区**。后续统一使用 LCE。旧称 `Region A` 和 `legacy type4 extent`仅作为历史名称保留，当前文档禁止继续使用。

## 分析笔记

仍有长期价值但不应作为当前状态表的专项调查放在 `audit/protocol/notes/`。这些笔记只能补充来源/边界，不能覆盖机器账本与标准文档。
