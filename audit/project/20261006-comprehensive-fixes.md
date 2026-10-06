# 全面审计修复跟踪

基线：[22 项审计](20261006-comprehensive-technical-debt-audit.md)。修复保留历史 EDP 盘协议及金标。

| 项目 | 状态 | 验证 |
|---|---|---|
| F2 验证路由 | 已实现 | 数据、构建脚本、工作流和未知路径闭环；首次推送及重命名覆盖 |
| F3 恢复身份 pin | 已实现 | raw-only serial 回归 |
| F1 GPT 采集 | 已实现 | 共享表头解析、8 MiB 分区项预算、每扇区一次读取并封存原始证据 |
| F9 恢复语义授权 | 已实现 | 完整原始元数据、区域语义、几何、无重叠、Plain 表/LCE 原始证据复核 |
| F10 写读预算一致 | 已实现 | artifact 数量/单项/总量边界；超大 manifest 拒绝发布 |
| F11 原子发布 | 已实现 | 私有候选文件、排他发布、文件同步/Unix 目录同步；Windows write-through 能力显式区分 |
| F12 不可变沿革 | 已实现 | 共享排他发布；并发同路径只有一个成功 |
| F14 私有文件 | 已实现 | Unix 文件 0600、新目录 0700、sudo 归属；Windows 受保护 owner/SYSTEM/admin DACL |
| F16 原子导出 | 已实现 | 稀疏镜像候选文件；Inspect 独立运行目录及含哈希的完成标记 |
| F4 无调用开发遗留 | 已实现 | 删除 14 个定义、未使用载荷 locator/stream 链及 DiskFacts.label_id；保留实时文件系统安全检查 |
| F5 单一计划状态 | 已实现 | RegionDisposition 是唯一保存状态；删除全局 filesystem_format |
| F6 v3 必需字段 | 已实现 | 目的、恢复契约、身份强类型必填；合法现有 JSON 不变 |
| F7 Inspect 上下文 | 已实现 | 删除无上下文入口及 LBA2/LBA9 猜测；缺少上下文保持 raw |
| F8 同步能力 | 已实现 | SectorDev.sync/reopen_rdwr 为必需实现；测试设备显式声明行为 |
| F15 必需金标 | 已实现 | 文件、长度、SHA 校验失败立即失败；删除跳过必需文件的分支 |
| F17 备份时间权威 | 已实现 | 验证后的 created_epoch 排序、保留与显示；未知时间明确标注；改名/touch 回归 |
| F18 扫描错误传播 | 已实现 | 删除打印后返回空列表入口；目录错误向 info/匹配调用链传播 |
| F13 秘密生命周期 | 已实现 | UTF-8 删除/取消/扩容擦除、直接转移分配、密码字段/队列强类型；提权返回码允许清理；新增 --prompt-passwords |
| F21 CLI 语法目录 | 已实现 | 密码选项声明敏感性及补全可见性；parser 受统一目录约束；四行输入与 argv 密码互斥 |
| F19 构建事实 | 已实现 | Git 定位 HEAD/index/packed refs、全跟踪文件 dirty 范围；提交时间驱动正式构建；工具链与平台配置共享 |
| F20 安装原子性 | 已实现 | 同目录候选验证、安装锁、原子替换；候选错误/路径遮蔽/发布后失败恢复旧版本回归 |
| F22 发布门禁 | 已实现 | main 同 SHA CI 严格成功、同 SHA 可复用 HIL、协议/依赖/格式检查；七套资产及 SHA/元数据闭环 |

22 项确认问题均已实施。审计末尾的低优先级候选仍是需测量或产品需求的后续方向，未用未经测量的改动宣称收益。

提交记录：`24bfc91`（F2/F3）、`874c216`（备份授权、预算、文件发布）、`5d248a0`（历史开发接口及类型收敛）、`0b29062`（目录时间/错误）、`7358f33`（秘密生命周期与敏感选项）。最后一批为构建事实、原子安装、同提交发布验证，以及远端 Windows 门禁发现的 UTF-8 文档读取修复。

保护边界：保留历史 EDP 物理盘解析与 20 个协议金标；EDPB 仅保留 v3。上一轮离线转换的现有备份继续使用，未访问物理盘。原审计 evidence/probe 是基线缺陷证明，其中旧断言、旧接口及范围清单不代表修复后代码；新回归纳入普通测试及 scripts/tests/test_engineering.py。

工程验证分别覆盖普通 clone、detached worktree、packed refs、文档修改/删除 dirty、无 Git 源码包；发布同 SHA/分支/事件/状态与缺失/损坏/多余资产；本地候选失败、路径遮蔽、并发锁及发布后回滚。CLI 新增 AST 检查，验证所有 parser 旗标分支与 catalog 的选项名称和参数形状一致；全局 --version 别名与内部提权/恢复通道明确分开，敏感参数不进入三种 shell 补全。

限制：设置 SOURCE_DATE_EPOCH 仅固定构建时间信息，本轮没有声称全部归档字节可复现。虚拟 HIL 仍只在独立工作流中运行；真实 USB HIL 未执行。跨平台编译与本地测试不能代替远端原生 CI/HIL 结果。

最终本地验证：fast 8 suites / 10 artifacts 零失败（31.96s）；full 同样覆盖 8 suites / 10 artifacts，加 doctest 零失败（32.21s）。Windows x86_64 全目标 Clippy（-D warnings）、Linux x86_64 全目标 check、cargo-deny 均通过；deny 仍报告上游链引入的 hashbrown/syn 多主版本警告，没有禁用规则或不兼容 patch。

五个改动工作流通过 actionlint 1.7.12（本次未调用 shellcheck/pyflakes）；协议基线 20 个唯一 LBA0-12 镜像通过（19 strict + 1 authentic）；仓库及用户目录的 121 份现有 EDPB v3 备份使用最终 reader 只读复核通过。

远端提交 7358f33 的 [Virtual Disk HIL](https://github.com/Evolution404/edpcli/actions/runs/37420791511) 通过；同提交 Rust CI 的 Windows 门禁因 Python 使用 cp1252 读取中文文档失败。本轮显式指定 UTF-8，随后本地 runner 回归通过；最终提交的远端验证将单独记录，未把上个提交的结果当作最终提交的绿色证据。
