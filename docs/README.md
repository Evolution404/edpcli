# edpcli 文档索引

本目录只保留**当前有效、可作为实现事实源**的文档。已完成或被取代的阶段计划、交接稿和重复审计已移除，Git 提交记录保存变更过程；`audit/` 保留仍用于验证的证据。

## 用户与发布

- [`user/USAGE.md`](user/USAGE.md)：安装、CLI/TUI 使用、备份与恢复。
- [`user/RELEASE.md`](user/RELEASE.md)：版本、构建、发布门禁。

## 当前架构

- [`architecture/ARCHITECTURE.md`](architecture/ARCHITECTURE.md)：当前模块边界、只读/写入安全边界、CLI/TUI/应用层/平台层关系。
- [`ui/TUI.md`](ui/TUI.md)：当前 TUI 工作区、键位、长操作进度、表格与安全交互规范。

- [`architecture/PHYSICAL_HIL_GOVERNANCE.md`](architecture/PHYSICAL_HIL_GOVERNANCE.md)：当前实体盘验收场景和证据要求；实体盘验收与虚拟磁盘门禁分开。

## 备份

- [`backup/EDPB_FORMAT.md`](backup/EDPB_FORMAT.md)：外层容器 1.0、当前 v3 清单与 `Artifact`/`Region` 恢复契约。

## EDP 协议

- [`protocol/README.md`](protocol/README.md)：协议文档阅读顺序与事实源规则。
- [`protocol/EDP_LBA0_12_FIELD_GUIDE.md`](protocol/EDP_LBA0_12_FIELD_GUIDE.md)：LBA0–12 人类可读字段手册；由机器字段目录生成/校验。
- [`protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md`](protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md)：协议逆向、证据、历史配置类型与来源信息的唯一总文档。
- [`protocol/LCE.md`](protocol/LCE.md)：LCE（LBA7 兼容扩展区）专项闭环与未闭环边界。

## 制盘

- [`provisioning/PROVISIONING.md`](provisioning/PROVISIONING.md)：当前五种目标、布局、数据保留/重建、密钥域、文件系统能力与写盘安全规范。

## 证据目录

协议机器账本、金标测试夹具、静态/虚拟/物理证据位于 [`../audit/protocol/`](../audit/protocol/)。`audit/` 是证据层，不是第二套产品文档。逐文件清理结论和备份迁移记录见 [`../audit/project/`](../audit/project/)。

## 文档规则

1. 当前行为只写入上述标准文档；不要再创建 `*_PLAN_*`、`*_AUDIT_*`、`HANDOFF_*`、`LIVE_STATUS` 等平行事实源。
2. 协议语义优先级：机器账本/字段目录/测试 > 协议总文档 > 专题说明；历史分析笔记不能覆盖标准结论。
3. 过程性调查若仍有长期证据价值，放到 `audit/protocol/notes/`，并明确它是来源/边界说明而非当前状态表。
4. 文档中的“已支持/已闭环”必须能指向代码和测试；未实现能力不得以计划语气伪装成当前功能。
5. 人类阅读的 Markdown 正文必须使用中文；只有代码标识、变量/字段/函数/类型名、命令、路径、协议枚举、原始二进制字符串和必要专有名词可以保留原文。
