# 剩余技术债审计与优化顺序 · 2026-10-06

基线：`1ad0b89ab6fa46e578ff2619806520fb6b09c11b`，版本 `2.5.0`。本轮审计与上一轮 EDPB v3 清理连续；本报告记录尚未实施的优化，不把既有历史审计结论当作当前代码事实。

## 结论

已移除的旧 EDPB 读取和开发兼容桥接没有重新出现。剩余值得优先投入的工作分成两类：先修复重复实现造成的边界差异，再删除已经没有消费者的开发遗留能力。单纯按文件大小拆分、按 `legacy/compat` 关键字删除不能代替调用链验证。

| 编号 | 优先级 | 已确认问题 | 建议 |
| --- | --- | --- | --- |
| F1 | P1 | GPT 备份二次读取绕过首轮数组预算 | 统一主/备 GPT 验证及不可变采集证据 |
| F2 | P1 | CI 分类漏掉协议夹具和仓库 EDPB 数据 | 补全路径路由与分类行为测试 |
| F3 | P2 | 原文身份生成 resume pin 时依赖预填摘要 | 统一现行 pin 摘要生成；保留提权摘要机制 |
| F4 | P2 | 6 个无调用常量/包装及无消费者文件载荷链 | 删除公开遗留入口和运行时 locator 投影 |
| F5 | P2 | `PartitionAction` 与 `RegionDisposition` 双状态 | 只保存 disposition，按需派生行为 |
| F6 | P2 | v3 必需字段仍建模为可缺省 Option | 使用必需字段，收紧合法状态集合 |
| F7 | P2 | 单扇区 Inspect 旧入口仍触发上下文猜测 | 测试迁到完整上下文；缺上下文明确 Unknown |
| F8 | P2 | SectorDev 写生命周期默认成功 | 显式区分读、写、同步能力，内存设备显式实现 |

其中 F1、F3 完成有界内存复现，F2 完成分类命令复现；其余是直接调用点、字段与实现核验。没有打开物理盘、修改备份或变更生产逻辑。

## F1 · GPT 的重复实现已经出现资源边界差异

[标准分区表解析](/Users/zhangyuxi/Desktop/edpcli/src/partition_table.rs:310) 限制 GPT 分区数组不超过 8 MiB；[备份镜像验证](/Users/zhangyuxi/Desktop/edpcli/src/backup_metadata.rs:201) 自行重新解析主/备表头，并在 [第二次数组读取](/Users/zhangyuxi/Desktop/edpcli/src/backup_metadata.rs:212) 只检查整数乘法、非零长度和 128B entry，没有同一大小上限。

`acquire_plain_metadata` 先运行 `read_partition_table`，随后重新读取主表头。稳定、超大首轮输入会被正确拒绝；问题触发条件是两次读取之间主表头变化。内存复现使用首轮 `entry_count=4`，第二轮改成 CRC 正确的 `entry_count=65540`，即 8 MiB + 512B 数组。第二轮已进入数组分配/读取，而不是在预算检查处拒绝；设备在首次数组访问即返回预设错误，避免大量 I/O。设备写入次数为 0。

这不是普通静态 GPT 都会触发的缺陷，也没有复现真实写盘损坏。它证明当前两个解析路径的输入边界不同，而且验证后的数据与最后归档的再次采集字节还存在分离。

**优化：** 把主/备表头、两份分区数组、几何及 CRC 验证合为共享的有界 GPT 采集结果；备份直接保存同一份已验证原始字节。近期至少给每次读取统一大小/地址检查，并校验再次读取与先前证据一致；后续消除重复读取与 `PlainGptHeader` 的平行模型。

**验收：** 首轮超预算、第二轮表头变化、数组变化、地址溢出、主/备不一致都在有界读取内失败；正常 MBR/EBR/GPT 的原始归档字节、恢复范围与 CRC 行为不变。复用 GPT 规则不能删去 EDP 盘历史协议解析。

## F2 · CI 的数据变更路由没有覆盖全部事实源

[分类规则](/Users/zhangyuxi/Desktop/edpcli/scripts/change_scope.py:8) 与 [CI 条件](/Users/zhangyuxi/Desktop/edpcli/.github/workflows/ci.yml:69) 直接确认：

```text
python3 scripts/change_scope.py tests/fixtures/protocol/mode1/aigo_u335_20260828_lba0_12.bin
rust=true, protocol=false, deps=false

python3 scripts/change_scope.py backup/example.edpb
rust=false, protocol=false, deps=false
```

第一种只跑 Rust 主门禁而不触发协议 gold 审计。第二种只剩 `repository-audit`，不会执行读取仓库 EDPB 的 backup suite 或主平台 full。上一轮修改因同时涉及 Rust 源码而通过了全套门禁；本问题针对将来仅改数据的提交。

**优化：** 将 `tests/fixtures/protocol/**`、`tests/fixtures/protocol_evidence/**` 纳入协议分类；把已跟踪 `backup/*.edpb` 纳入 Rust/备份相关门禁。完整协议基线继续使用 `audit/protocol` 作为权威总体，不把所有夹具强行加入指定 gold。给具体数据路径、重命名和删除添加分类测试。

**验收：** 协议样本单独变更会触发协议审计；仓库 EDPB 单独变更会触发备份校验套件。仅文档变更仍按既有分级运行，不恢复无差别 CI。

## F3 · 现行身份 pin 仍有双份序列号事实

[ResumePin 构造](/Users/zhangyuxi/Desktop/edpcli/src/media_identity.rs:188) 直接复制 `hardware.serial_sha256`；[内存 pin 校验](/Users/zhangyuxi/Desktop/edpcli/src/media_identity.rs:283) 已能从原文计算摘要。当前 v3 canonical identity 恰好保存原文、摘要为 None。

复现：构造 `serial=Some("AUDIT-RAW-SERIAL-001")`、`quality=Usable`、`serial_sha256=None` 的身份。`MediaIdentityPin::verify` 成功；`MediaIdentityResumePin::from_pin(...).validate()` 返回 `invalid identity pin digest`。普通原生探测目前同时生成原文和摘要，因此该复现不意味着每次 TUI 提权都失败；它揭示公共构造路径对内部缓存字段的隐含依赖。

**优化：** `from_pin` 复用同一现行摘要生成逻辑。进一步将观察到的原文身份和可序列化的恢复 pin 分成不同模型，避免原文/摘要被独立修改后冲突。

**验收：** 原文身份及现行观察身份都可生成合法 pin；序列号变化继续拒绝；argv/lineage 不包含序列号原文。这里的摘要是提权安全机制，不能当作旧 EDPB 摘要兼容一并删除。

## F4 · 无调用公共入口与文件载荷遗留

全库搜索后逐项核验，以下定义没有运行时、测试、示例调用：

| 定义 | 位置 | 现状 |
| --- | --- | --- |
| `PARTITION_PREFIX_SECTORS` | [backup_metadata.rs](/Users/zhangyuxi/Desktop/edpcli/src/backup_metadata.rs:23) | 旧深度采集前缀配置，已无消费者 |
| `PARTITION_SUFFIX_SECTORS` | [backup_metadata.rs](/Users/zhangyuxi/Desktop/edpcli/src/backup_metadata.rs:24) | 旧深度采集后缀配置，已无消费者 |
| `FORMAT_NAME` | [edpb/model.rs](/Users/zhangyuxi/Desktop/edpcli/src/edpb/model.rs:4) | 与 EXTENSION 重复的未使用常量 |
| `IDENTITY_HEADINGS` | [application/identity.rs](/Users/zhangyuxi/Desktop/edpcli/src/application/identity.rs:11) | 未被表格 schema 或渲染使用 |
| `summarize_backup` | [metainfo.rs](/Users/zhangyuxi/Desktop/edpcli/src/metainfo.rs:438) | 当前消费方复用 verified reader，旧路径包装无人调用 |
| `write_core_backup_with_notes` | [edpb/write.rs](/Users/zhangyuxi/Desktop/edpcli/src/edpb/write.rs:250) | 仅定义和重导出，没有调用 |

这些入口因公开或经 glob 重导出，普通 `dead_code` 门禁并不能证明它们有消费者。

此外，[stream_file_payload](/Users/zhangyuxi/Desktop/edpcli/src/filesystem/analysis/mod.rs:102) 没有调用方；仓库外部唯一同名 token 是架构测试的禁止调用字符串。`FilePayloadExtent/Locator/PayloadReadSummary` 与 `FileEntry.payload_locator` 只为该能力维护。FAT/exFAT 分析器仍构造文件簇位置、额外 Vec 和 extent 投影，但没人消费结果。

**优化：** 先删除表中 6 个低风险定义及导出；随后删除载荷流读取函数、locator 类型/字段和 FAT/exFAT 的 locator 构造。保留簇链循环/越界/交叉分配检查、文件容量检查、目录统计、格式化空盘验证和现行文件系统分析。

**验收：** 无孤立公开遗留入口；FAT/exFAT 分析 JSON 与格式化验证结果不变，仍拒绝损坏簇链。这里不能删除整个 `filesystem::analysis`：`application/provision/commit/partition_format.rs` 仍依赖它验证空文件系统。

## F5 · 制盘计划双状态容易漂移

[TargetPartitionPlan](/Users/zhangyuxi/Desktop/edpcli/src/provision/reprovision/plan.rs:66) 同时保存 `action: PartitionAction` 和 `disposition: RegionDisposition`。`legacy_action()` 将保留/重新包装映射到 PreserveExact、其余映射到 Rebuild；创建时派生一次，`force_rebuild_for_format` 再手动更新两份状态。范围保留、导出和部分校验仍读取 action，其他安全校验和 UI 读取 disposition。

这是开发模型演进遗留，真实 EDP 协议格式并不依赖这两套 Rust enum。

**优化：** 移除持久化 action 和 legacy_action，`preserves_extent()` 成为统一判断；若旧二值行为确有消费需要，用只读 getter 派生，不存第二份状态。迁移 CLI/TUI、内存测试、真实 HIL 示例的消费方，保持原有分区保留和格式化授权规则。

**验收：** 计划无法出现“action 保留、disposition 重建”这种矛盾组合；mode0～3、Plain、密钥重新包装和明确重建的扇区写集合及只读展示保持一致。

## F6 · EDPB v3 必需字段仍保留旧可缺省建模

[Manifest](/Users/zhangyuxi/Desktop/edpcli/src/edpb/model.rs:240) 的 `backup_purpose`、`restore_contract`、`identity` 仍是带 serde default 的 Option；[验证器](/Users/zhangyuxi/Desktop/edpcli/src/edpb/validate.rs:3) 和 identity 校验随后强制要求它们存在。合法 v3 全都具备这些字段，因此 Option 只增加可构造的非法状态及重复判空路径。

**优化：** 将这三项改成必需类型，schema 收口为单一 v3 表达；保持当前合法 v3 的 JSON 键值和外层容器 1.0，不设计另一轮不必要的格式迁移。`Core`、空 partitions、可缺失的真实硬件事实仍有当前用途，不能跟着删掉。

**验收：** 缺必需字段在反序列化时失败，所有现有合法 v3 和本地 121 份容器保持可读；身份投影冲突与 metadata_only 契约检查继续有效。

## F7 · Inspect 的开发期缺上下文回退仍存活

[analyze_sector](/Users/zhangyuxi/Desktop/edpcli/src/inspect/lba_adapter.rs:3) 只包装 `analyze_sector_with_context(..., None)`，实际消费集中在测试。CLI/TUI 的应用层均使用带上下文入口。LBA2 缺 LBA1 时仍以全零/非零猜 GPT profile，LBA9 缺 LBA6 时仍采用 short/non-long-user 回退；它们是工具 API 的上下文兼容，不是历史真实介质 profile 的读取支持。

**优化：** 测试和示例转到带上下文入口；删掉无上下文包装与仅为该路径保留的猜测。确实缺伴随扇区的输入返回 MissingProtocolContext/Unknown 和原始字节，不推断 profile。当前领域解析器和历史真实 profile 仍完整保留。

**验收：** 完整上下文的新旧盘字段/解密结果不变，损坏或缺少上下文时不把猜测展示为事实；真实 gold/profile 测试继续全量运行。

## F8 · 写能力默认成功属于接口债

[SectorDev](/Users/zhangyuxi/Desktop/edpcli/src/ports.rs:19) 的 `sync`、`reopen_rdwr` 默认返回成功，方便内存设备，但也使遗漏实现的适配器具备表面成功的写生命周期。当前生产 `FileDev` 显式实现真实重开与同步；没有发现它在使用默认成功。

**优化：** 短期去掉默认成功，要求内存/故障设备显式实现；中期在 ports 统一读能力与写能力，避免另造与现有 `SectorReader/PartitionReader/FilesystemReader` 平行的读取接口。读接口的 u64 地址与写接口目前 u32 限制应分别表达，不顺手放宽实盘写范围。

**验收：** 新写设备遗漏 sync/reopen 会编译失败；只读消费者不获得写能力；真实同步、重开失败、身份复核、锁定 guard 和回滚测试继续通过。

## 低优先级文档与保留边界

- 架构总述仍宣称“历史 EDPB 读取兼容单独维护”，与同文 v3-only 章节冲突。本轮将总述修正为保护 EDP 介质历史协议、仅接受 EDPB v3。
- `protocol/crypto.rs:226` 仍提到已删除的 u32 兼容包装；仅需修正文注，不改变运算实现。历史审计报告应保留时间语境，不按关键词批量改写。
- 一行 application 门面仍是活跃导入边界，不能仅因重导出就判定为死代码。`backup_prepared_provision_on_disk` 即使没有产品内直接调用，仍是构造公开不可伪造备份证明的配套 API，有可编译文档；没有列入直接删除清单。
- 所有 112 份 tests Rust 文件都可从 10 个 Cargo 测试根的 path 模块图找到，没有确认到未注册测试。源树的 402 份 Rust 文件（包含内部单测）及单文件过千行本身不构成删除证据。
- 原生平台探测回退、提权摘要、LCE/LBA7、legacy-v0064、LBA6 历史 MBR、LBA4 身份/密钥、LBA12 历史布局与全部协议证据继续保护。

## 推荐实施批次

1. F1/F2：先统一 GPT 预算与采集证据、补 CI 数据路由，独立验证并提交。
2. F3/F4：修正 pin 构造，删除无调用常量/包装和载荷投影；保留文件系统分析验证。
3. F5/F6：分别收敛制盘状态、v3 必需字段；每项单独提交，避免把格式与写计划模型变化混在一起。
4. F7/F8：收敛 Inspect 上下文与设备能力契约，迁移测试/HIL 示例后做完整跨平台验证。

各批次沿用 fast/full 和协议门禁；宽范围模型重构执行 900 秒预算 full runner。每批及时提交推送。没有理由为这些清理重新加入 EDPB v1/v2 运行时兼容，也没有理由删除真实 EDP 盘历史支持。

## 本轮验证记录

- `cargo build --locked --lib`：成功，用其实际产物链接临时内存诊断程序。
- 两个内存复现均达到断言并成功退出：GPT 重读预算差异、raw-only pin 构造差异；未调用物理盘。
- CI 分类命令复现成功；调用搜索、路径模块图和字段更新点完成核验。
- `scripts/test-fast.sh`：格式/差异检查、table-scroll、3 套件 / 5 构建产物通过，0 失败，runner 30.87 秒。文档修改没有 Rust 输入变化，因此 runner 按规则跳过 Clippy。
- 本报告及架构总述修正提交推送；具体 Git 结果记入会话进度日志。本报告是后续优化方案，生产修复尚未实施。
