# LCE 审计证据

LCE 表示“LBA7 兼容扩展区”。本目录是 LCE 以及指向它的旧版 EDP 分区表条目的标准机器可读证据集。

统一术语：

- 元数据对象：`LBA7 legacy EDP partition table`
- 一方结构体：`tagEdpPartionInfo` / `EDP_PARTION_INFO`
- 逻辑类型：`1=boot`、`2=share/exchange`、`4=encrypt/private`
- 物理对象：LCE（LBA7 兼容扩展区）
- 负载大小：`0xC00` 字节（6 个 512 字节扇区）
- 新格式对应结构：LBA12 `tagNewEdpPartionInfo`

LCE 不专属于 type4。一方 `CreatePartitions` 会保留后续条目的逻辑 `PartionType`，但把其几何改写为同一个固定兼容扩展区。因此模式 0 `[1,2,4]` 中 type2 和 type4 合法地指向同一个六扇区块；模式 3 `[1,2]` 也可以由 type2 指向该区域。

文件：

- `evidence/official_lba7_producer_20260923.json`：由写入端得到的模式矩阵、类型语义和条目几何规则。
- `byte_ledger.tsv`：精确覆盖 3072 字节负载。
- `profile_coverage.tsv`：已验证的物理配置类型覆盖。
- `evidence_manifest.tsv`：证据身份、摘要和适用边界。
- `gold/`：已提交的物理密文和恢复明文测试夹具。
- `live_captures/`：有边界的只读物理采集。

IIR 是独立协议对象，有意不建模为该扩展区。

LCE 的底层写入端、位置、负载、加解密、旧版消费端和驱动 I/O 链已经闭环。剩余开放来源问题是：哪个上层业务触发在何时、为何决定重写 LCE，以及更早写入端版本是否等价。详见 `docs/protocol/LCE.md`。
