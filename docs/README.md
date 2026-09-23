# edpcli 文档索引

本目录只保留**当前有效、可作为实现事实源**的文档。阶段计划、交接稿、实时进度稿和已完成审计不再长期保留；历史过程由 Git 提交记录承担。

## 用户与发布

- [`user/USAGE.md`](user/USAGE.md)：安装、CLI/TUI 使用、备份与恢复。
- [`user/RELEASE.md`](user/RELEASE.md)：版本、构建、发布门禁。

## 当前架构

- [`architecture/ARCHITECTURE.md`](architecture/ARCHITECTURE.md)：当前模块边界、只读/写入安全边界、CLI/TUI/application/platform 关系。

## 备份

- [`backup/EDPB_FORMAT_V1.md`](backup/EDPB_FORMAT_V1.md)：EDPB v1 容器与 Artifact/Region 规范。
- [`backup/DEEP_BACKUP_V1.md`](backup/DEEP_BACKUP_V1.md)：Deep Backup 的只读文件系统 inventory、解密和证据边界。

## EDP 协议

- [`protocol/README.md`](protocol/README.md)：协议文档阅读顺序与事实源规则。
- [`protocol/EDP_LBA0_12_FIELD_GUIDE.md`](protocol/EDP_LBA0_12_FIELD_GUIDE.md)：LBA0–12 人类可读字段手册；由机器字段目录生成/校验。
- [`protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md`](protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md)：协议逆向、证据、历史 profile 与 provenance 的唯一总文档。
- [`protocol/LCE.md`](protocol/LCE.md)：LCE（LBA7 Compatibility Extent）专项闭环与未闭环边界。

## Provisioning

- [`provisioning/PROVISIONING.md`](provisioning/PROVISIONING.md)：Provisioning 的唯一长期事实源，同时记录 CURRENT 已实现能力与未来官方四模式/转换 ROADMAP，并严格区分两者。

## 证据目录

协议机器账本、gold fixture、静态/虚拟/物理证据位于 [`../audit/protocol/`](../audit/protocol/)。`audit/` 是证据层，不是第二套产品文档。

## 文档规则

1. 当前行为只写入上述 canonical 文档；不要再创建 `*_PLAN_*`、`HANDOFF_*`、`LIVE_STATUS` 等平行事实源。
2. 协议语义优先级：机器 ledger/catalog/test > 协议总文档 > 专题说明；历史分析笔记不能覆盖 canonical 结论。
3. 过程性调查若仍有长期证据价值，放到 `audit/protocol/notes/`，并明确它是 provenance/边界说明而非当前状态表。
4. 文档中的“已支持/已闭环”必须能指向代码和测试；未实现能力不得以计划语气伪装成当前功能。
