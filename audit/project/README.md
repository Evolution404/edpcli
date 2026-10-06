# 项目架构审计索引

审计基线：[2026-10-06 全仓库技术债与优化空间审计](20261006-comprehensive-technical-debt-audit.md)。它合并此前剩余 F1～F8，并补充 F9～F22、覆盖范围、删除/保护界线及验收顺序。

22 项确认问题的实施与验证见 [全面修复记录](20261006-comprehensive-fixes.md)。

当前审计证据位于 [evidence/](evidence/)；[范围清单](evidence/20261006-scope-inventory.tsv) 固定基线提交的文件名、大小及 SHA-256，用于区分“当时被审计的内容”和后续代码。复现程序是基线缺陷的证明，修复后其中成功/失败断言应相应变化，不属于常规生产测试入口。

历史报告按各自基线阅读，不把已完成的改动重复计为当前待办，也不删除原始协议/硬件证据：

| 报告 | 用途 |
| --- | --- |
| [整体架构审计](20261006-overall-architecture-audit.md) | 初始问题与原优化路线 |
| [S0 实施](20261006-architecture-s0-implementation.md) | 已实施写入安全边界 |
| [S1～S4 实施](20261006-architecture-s1-s4-implementation.md) | 已实施架构收敛 |
| [开发兼容清理](20261006-development-compatibility-cleanup.md) | 历史 API/包装清理结果 |
| [EDPB v3 清理与迁移](20261006-edpb-v3-only-migration.md) | 旧 reader 移除及本地转换/归档结果 |
| [剩余 8 项审计](20261006-remaining-technical-debt-audit.md) | 当前 F1～F8 的详细初次证据；待办以全仓库报告为准 |
| [2026-10-05 架构优化进度](20261005-architecture-optimization-progress.md) | 当时实施进度 |
| [2026-10-05 桌面 TUI 审计](20261005-desktop-tui-engineering-audit.md) | 当时 TUI 审计 |
| [2026-10-05 桌面 TUI 完成记录](20261005-desktop-tui-engineering-completion.md) | 对应实现结果 |
| [2026-10-05 工程 TUI 完成记录](20261005-engineering-tui-completion.md) | 对应实现结果 |
| [2026-10-05 独立质量复核](20261005-independent-quality-review.md) | 当时质量结论 |
| [2026-10-05 公共 API 兼容记录](20261005-public-api-compatibility.md) | 历史决策；后续开发兼容清理按用户新要求执行 |
| [2026-10-05 容量地图修复](20261005-capacity-map-visibility-fix.md) | 对应修复结果 |

当前实现架构以 [ARCHITECTURE.md](../../docs/architecture/ARCHITECTURE.md) 为准；协议历史材料继续由 audit/protocol 和 docs/protocol 管理。运行时进度日志规则在 [ai-progress/README.md](../ai-progress/README.md)。
