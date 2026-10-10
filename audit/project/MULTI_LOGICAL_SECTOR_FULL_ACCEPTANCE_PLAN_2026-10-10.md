# edpcli：全逻辑扇区规格、全模式和全工作流实施与验收计划（执行中）

## 2026-10-11 01:52 CST A03 v5 独立元数据恢复契约 / HIL 验收

- **已实现版本分界，不静默升级**：512B v3 正式恢复规则不变；1024/2048/4096B v4 继续仅 `EvidenceOnly`，拒绝伪造 `Restorable`；v5 是独立 `edpb.manifest.v5` 的“仅元数据可恢复”契约，禁止宣称文件系统或用户数据恢复。普通备份写入器继续发 v4，只有 `ci-virtual-disk` 特性可调用独立的测试专用 v5 文件写入器；**该入口仅创建 EDPB 文件，不授权对任意磁盘写入**。
- **白名单和结构硬门禁**：v5 必须具备完整原生 LBA0–12、完整 LCE、每个分区恰好一个原生首块 `raw.partition_header.N`，首块 LBA 必须等于备份分区表起点，不得夹带未知 `raw_sectors` 扇区、重叠范围、错误块大小、缺失/不完整 artifact 或 `restores_user_data`/`restores_filesystem`。旧的可恢复 Extent 白名单已严格扩充为“仅 v5 对应分区首块”的例外；v4 对同类输入仍拒绝。
- **真实 macOS HIL**：1024B、2048B、4096B 临时 512MiB Disk Image 的正式原生 Mode0 CLI/WAL、备份及独立重挂读取通过；将实际经核验的 v4 原生 LBA0–12、LCE、分区首块逐字节生成新的测试 v5 文件，`VerifiedBackupReader` 校验成功且与 v4 每个 artifact 字节完全一致，**3/3 通过**。1024B 进一步通过 v4/v5 只读预览的完整 LBA 顺序和写集 SHA256 一致性，交叉版本调用均被拒绝。原有 v4 测试专用元数据破坏→TargetSession/WAL 恢复→独立原生读取也继续通过。
- **单测和异常**：1024/2048/4096B 的 v4 伪造恢复权限、v5 缺失 LCE、分区首块、错误位置、不允许的数据 extent、不完整协议、错误扇区、Plain 伪装和用户数据/FS 恢复声明均必须被拒绝。第一次实际 v5 测试中旧白名单拒绝分区首块，已收敛成上述精确映射后复测通过。
- **仍未完成**：v5 正式备份发行策略、实体盘写前身份及同几何复核、显式独立恢复授权、断电/中断/错误回滚的真实 OS HIL、TUI PTY 和生产恢复入口均未开放。**v5 “可恢复契约能校验”不等于“已允许实体盘恢复”**；现有生产 restore 继续仅支持 512B v3。


## 2026-10-11 01:00 CST A06 Plain→Plain 默认无损保留验收

- **行为修复**：`src/application/provision/native_flow.rs` 的 Plain 来源→默认 Plain 目标，现在仅当已有单一 MBR P1（LBA2048 起、占满剩余原生盘）、当前 LBA0 与默认输出逐字节一致、来源原生文件系统确认为 exFAT，且没有显式自定义目标分区时，才将目标置为 `PreserveVerified`，原生事务写集仅保留未变化的 LBA0；否则保持原有重建/格式化路径。统一 `project_native_impact` 允许可信 PlainMbr 分区映射到同几何 Plain 目标，不再因目标没有 EDP 角色而强制报告数据丢弃。新增 4 规格定向单测检查同几何保留、错几何拒绝、写集覆盖拒绝、明确格式化即来源丢弃。
- **真实 OS 验收**：在 `512/1024/2048/4096B` macOS 临时 Disk Image 上使用正式 CLI/WAL 连续执行 Plain→Plain，各规格第二次只写 1 块、规划显示“来源数据保留：普通分区P1”“目标格式化：无”；首次写入的 `edpcli-native-hil.txt` 在再次转换及卸载重挂后内容完全保留，**4/4 通过、退出码 0**。HIL 矩阵首次 Plain→Plain 目标回读时现在必须验证用户文件存在且内容一致，避免再把重新格式化后可挂载误判为无损。
- **门禁**：`cargo fmt --all -- --check`、`git diff --check` 通过；`uv run --locked python scripts/test-full.py --profile full`：8 套、10 产物、0 失败；新增定向单测通过。GitHub 提交：`a2b6b58`、`e8f18fb`、`20882ac`，已写入目标开发分支。**Mac 本地 Git 当前因 GitHub SSH/HTTPS 连接失败仍停在 `60a4737b`，通过修订工作区镜像实测上述三个提交的代码；不能声称本地与远端已同步，禁用 reset/clean 并等待可正常 fetch 后对齐。**
- **范围**：证明默认精确 Plain→Plain 对已有普通文件不破坏；没有证明自定义 FAT/exFAT 分区配置变更无损。EDPB v4 仍 evidence-only，正式恢复授权、失败注入、TUI 真交互仍属下阶段。

## 2026-10-10 22:47 CST P2 100/100 原生 OS 虚拟块设备模式转换验收

- **四种原生逻辑扇区 × 五种来源 × 五种目标全部通过**：512B、1024B、2048B、4096B 各 25/25，共计 **100/100，失败 0**。四个独立 macOS 512MiB 临时 Disk Image 测试进程均返回退出码 0，所有目标组合通过正式 `edpcli provision write --include-virtual`、原生 TargetSession/WAL、弹出重挂、独立 LBA7/LBA12/LCE、密钥解封装与文件系统读取。每组先实际制备来源模式并验证，再执行目标转换和重新读回；同一规格的 25 组复用该规格专属临时 Disk Image。
- **可复现自动化**：提交 `f255b845` 的 `scripts/ci/macos-cli-native-virtual-hil.sh` 新增 `EDPCLI_HIL_PAIR_MATRIX=1`，默认遍历来源/目标 `plain mode0 mode1 mode2 mode3`，可分别用 `EDPCLI_HIL_MATRIX_SOURCES`、`EDPCLI_HIL_MATRIX_TARGETS` 指定子集。每个规格单独运行 `EDPCLI_HIL_PAIR_MATRIX=1 EDPCLI_HIL_SECTORS=512|1024|2048|4096 scripts/ci/macos-cli-native-virtual-hil.sh`；矩阵模式自动禁止测试专用改密分支，避免密码状态泄漏。独立临时磁盘均由 `hdiutil` 创建并确认 BusProtocol 为 Disk Image，非实体 USB。
- **严格区分分区无损承诺**：100/100 证明正式源/目标链路可提交和正确读取目标分区，**不能等同于全部100种情况的来源字节保留 SHA256**。既有 4 规格 Mode0→Mode1→Mode2 保密区完整范围+FileKey SHA256 和 Mode0→Mode0 自身全范围保留验证仍属独立证据。实体介质拔插、断电故障、TUI 交互、EDPB v4 正式恢复授权均未本轮验收。
- **待跟进的 Plain 语义**：读源码 `src/application/provision/native_flow.rs` 中 `ProvisionRequest::Plain` 无条件调用 `plan_native_plain_image`，因此默认 Plain→Plain 即使参数未改变，也重新格式化普通分区；当前矩阵验证的是格式化之后可挂载、不是 Plain→Plain 无损。进入 A06 统一来源数据丢弃/保留与必需格式化策略审计，不擅自把有破坏性行为改成“保留”。


## 2026-10-10 21:38 CST P2 正式 CLI 虚拟盘跨模式验收增量

- **正式 OS 模式链**：512B 和 1024B macOS 临时 Disk Image 完成 Plain→Mode0→Mode1→Mode2→Mode3→Plain，使用生产 CLI 写盘、同一 TargetSession/WAL 与每次卸载重挂的独立协议、LCE、加密卷与文件系统读回；所有节点成功。单项自动改密测试必须与连续模式链隔离：新增 HIL 环境参数 `EDPCLI_HIL_SKIP_CUSTOM_PW=1`，以免上次目标密码泄漏至下一来源。
- **跨模式真正无损保存**：新增只读 HIL `native_os_cross_mode_encrypt_full_extent_sha256`，在 Mode0 来源保存 Encrypt 保密区全部原生字节 SHA256、起点、扇区数及独立解封装的 FileKey 指纹，再经过 Mode1 和 Mode2 两次正式原生 WAL 转换后复核。**512/1024/2048/4096B 四规格全部通过**；并非只比对首扇区、协议表或“数据保留”的 UI 文案。执行：`EDPCLI_HIL_SKIP_CUSTOM_PW=1 EDPCLI_HIL_VERIFY_CROSS_PRESERVE=1 EDPCLI_HIL_MODES='mode0 mode1 mode2' EDPCLI_HIL_SECTORS='512 1024 2048 4096' scripts/ci/macos-cli-native-virtual-hil.sh`（已逐规格分别执行）。
- **追加 P2 后半段**：2048B、4096B 临时 macOS Disk Image 从 Mode2→Mode3→Plain，采用正式 CLI/WAL，验证了保密区来源丢弃、启动区和交换区重建，以及 Plain exFAT 完整原生块读回和卸载重挂；全部通过。结合前述两规格完整链与四规格 Mode0→Mode1→Mode2 全范围 SHA 验收，**四规格均覆盖 Plain→Mode0→Mode1→Mode2→Mode3→Plain 的各相邻边**（2048/4096B 的前后边由独立会话/不同临时盘验证，不能虚称一次连续全链）。
- **历史阶段范围（已由上方 22:47 的 100/100 验收更新）**：本节早期仅验证 Mode0→Mode1→Mode2 保密区原样保留，以及 Mode2→Mode3、Mode3→Plain 实盘形态虚拟块设备重建。现已补齐四规格全100组合的 OS 可读写验收；所有组合的全范围保留 SHA256、TUI 风险提示及格式变化故障注入仍待完成。EDPB v4 生产恢复权限仍处于 `EvidenceOnly` 未授权状态。


## 2026-10-10 21:16 CST P0 → P1 原生 EDPB 真正 OS 块设备实证回执

- **P0 身份识别卡点已修复**：原先原生虚拟盘 Writer 使用 `native_provision_probe`，而取证 `media_identity_from_protocol_image → identify` 使用未归一化原始探针；在 1024B Disk Image 上实际制盘成功、备份 `device_id` 校验失败。调整 `identify::generate_candidates` 与 `media_identity_observer::merged_hardware_probe`，仅对明确启用并且系统确认 `Disk Image` 的虚拟设备使用与正式规划一致的协议身份，**绝不为普通 USB 生成合成身份**。真实 1024/2048/4096B macOS OS 虚拟盘均已通过正式备份调用、EDPB v4 verify、完整原生 LBA0–12/LCE/分区首块双句柄比对和重挂后读回。提交 `2ae71495` 已建立。
- **P1 EDPB v4 原生恢复技术验证三规格通过（限 HIL）**：新增仅在 `ci-virtual-disk` 特性下编译的 `materialize_native_restore_evidence_for_hil`，从已验证 v4 证据生成全原生块写集并复核 SHA256、重复 LBA、LBA0 提交末位。新增 `tests/hil/native_edpb_wal_hil.rs`：限定独立 `diskutil` 核验的 512MiB 临时 Disk Image，用**正式** `TargetSession → native WAL` 修改 LBA8，回读确认变化；用备份的原生写集通过**同一** WAL 入口恢复，再独立打开原生块回读所有协议、LCE 及分区首块；卸载、重挂后调用官方模式/密码/FS 独立验证。**1024B、2048B、4096B 分别通过**；不是单纯内存模拟。
- **不扩大证据范围**：v4 manifest 仍标记 `RestorePolicy::EvidenceOnly`，普通编译没有 HIL 恢复材料化入口，`restore_plan.rs` 仍拒绝 v4；此轮未完成正式 v4 可恢复版本契约、实体介质恢复授权、断电注入或所有模式转换。512B v3 恢复继续原有行为。
- **本阶段下一步**：修订验证后的代码完成 Fast/Full Gate、提交并推送；随后建立可版本化的正式 v4/v5 恢复授权契约及真实断电故障模拟；继续 Plain/Mode0..3 × 四规格真实来源→目标模式保留/格式化矩阵和 TUI 交互验收。无实体盘写入，无 reset/clean，GitHub CI 仍延后 main 合并。


## 2026-10-10 20:50 CST 本轮实施与验收回执（优先于下方旧状态表）

- **P0 A01/A05/A02 已完成代码和 Fast Gate**：由并行会话提交并推送 `bd6131e0`，四规格来源密码判断、Native 实际算法、来源硬件/序列号/完整协议及必要分区首块/LCE 写前复核。单元故障注入通过；**尚未进行真实换盘/拔插故障注入**。
- **P1 A04/A08/A11 已交付正式路径迁移**：`4cd6782e` 已推送。CLI 来源认证计划先只读验证 EDPB，再进入同一 Native planner；移除无正式调用的 TUI 4Kn Mode1 专用任务/结果态，净删除约 264 行。历史 Mode1 离线镜像 API 仍有离线消费者，不按冗余代码直接删除。来源重放的标准逻辑扇区判断复用 Registry；修正文档及镜像文案。Fast Gate 8 套/10 产物/0 失败；CLI 112/112、TUI 461/461 通过。
- **小容量原生几何修复**：`c95a321f` 本地提交。512MiB 目标上原来的默认 1024MiB 新保密区无法容纳；仅新建目标按可用空间选择默认容量，已有来源分区起点和容量绝不调整。四规格回归及原有 25 组合几何测试通过；真实 macOS Disk Image 上 512/1024/2048/4096B Mode0 制盘、重挂、密钥解封装和 LCE 独立检查通过。
- **A07 四规格 Mode0 全范围无损保留 + 仅改密正式 HIL**：同规格 Mode0→Mode0 无改动、再用独立交换/保密密码 Rewrap；四种原生块大小在经 `diskutil` 验证的临时 512MiB Disk Image 上执行正式 CLI `write`/WAL，重挂后从原生完整分区范围流式 SHA256，比对启动区、交换区、保密区、完整 LCE 原生块及原 FileKey，全部一致。验收测试：`EDPCLI_HIL_PRESERVE_FULL=1 EDPCLI_HIL_MODES=mode0 scripts/ci/macos-cli-native-virtual-hil.sh`，已按规格独立执行。批量首次因上一规格测试密码泄漏误报 PasswordMismatch；未改生产代码，随后分别复核通过。**仅证明 Mode0 等几何保留与 Rewrap；不代替其他24种转换组合的全范围保留证据。**
- **A03 EDPB**：已新增 1024/2048/4096B v4 同几何身份/容量/原生 LBA0–12/LCE 完整字节的**只读恢复预览**与校验摘要；CLI `provision restore-preview --disk N --backup FILE.edpb [--include-virtual]` 只输出计划并拒绝恢复授权。三规格一次验证容器和模拟 Native WAL 完整恢复、失败回滚测试通过，LBA0 最后提交且未知尾部保留；512B v3 原有恢复路径不改。**v4 仍是 evidence-only；尚未实现经授权的 macOS OS 虚拟块设备真实 EDPB WAL 恢复和独立重挂检查，不能标记 A03 完成。**
- **A09 结果语义修正**：正式 Native 结果投影现在沿用 Application 规划的实际 `RegionDisposition`，不再无条件显示为 Rebuild；四规格 Preserve/Rewrite/Rebuild 定向单测通过。WAL 与 EDPB 的类型化结果拆分仍待收口。
- **完整 Rust 回归**：`project_validate` unfiltered cargo tests 最终 885 通过/0 失败，覆盖 CLI/TUI/Backup/Provision 及文档测试；最初发现的 2 个 TUI 默认 Quick 模式回归已修复并定向复测。此结果与独立 OS HIL 分开记录；项目自有 `uv ... test-full.py --profile full` 和冗余审计尚未在本轮获得完成回执。
- **继续工作顺序**：v4 有版本可恢复契约及同几何正式虚拟 WAL 恢复 → 各模式部分保留/变化强制格式化的真实 HIL → TUI PTY 全流程 → WAL 断电/失败注入、1024B 偶发中断归因 → `--include-virtual` 发现对象化与正式/离线旁路清理 → Full Gate、冗余审计、文档/合并 main。不得跳过未完成门禁或把内存模拟称为 OS 块设备恢复；CI 最终合并 main 再查。


## 2026-10-10 全局架构审计后执行队列（最新权威）

**真相源与优先级**：本节结合 [`GLOBAL_ARCHITECTURE_AUDIT_2026-10-10.md`](GLOBAL_ARCHITECTURE_AUDIT_2026-10-10.md) 的 A01–A14 编制，覆盖下方旧 S0–S6 和“历史基线”的任何过时推断。审计提交 `85a023ea` 已推送至 `feat/native-4kn-wal-staging-20261010`；先前实现提交 `9fc54cc1`（默认保留）、`9440ce75`（三规格原生 EDPB 取证）和 `b57610ad`（统一 EDPB 只读核验）均已在此分支。审计时 `scripts/test-fast.sh`：8 suites/10 artifacts/0 failures，冗余扫描 851 文件、0 确认缺陷、216 **待裁决候选**，不要直接删除候选。

**并行工作保护（2026-10-10 20:05 CST 核对）**：工作区有**另一 AI 尚未提交**的 `src/application/provision/native_flow.rs`、`src/tui/controller/tests.rs`、`src/tui/native_source_password_policy.rs`、`src/tui/provision/review.rs` 改动，正在执行 A01/A05 相关测试。下一 AI **首先重新执行** `git status --short --branch`、`git log -5 --oneline`、`git diff --stat` 和进程/任务核对，阅读这些 diff、确认是否已有提交；**保留这些改动，不要 reset/clean/stash/覆盖，也不要重复抢改**。该阶段当前记为 **进行中／未验收**，不能把测试启动当成功。

| 顺序 | 审计项 | 应完成的代码改造 | 必须通过的门槛 | 现况 |
| --- | --- | --- | --- | --- |
| **P0-1** | **A01 + A05** | 将 TUI 来源密码验证白名单由 512/4096B 扩为与 Application 一致的 512/1024/2048/4096B；Native 计划带出**真实算法**供 TUI 确认页显示，而不是 `None`。明确区分 FileKey 封装算法与分区加密算法。 | 四规格密码正确/错误、Unknown→opaque、来源未变、算法确认页相符；8192B 非法 FS 不意外放行；相关 TUI/CLI 回归通过。 | **其他 AI 正在改，未验收/未提交** |
| **P0-2** | **A02** | 将规划时硬件识别 `MediaIdentityPin`、设备身份/VID/PID/序列号、逻辑几何、完整 LBA0–12 及必要 LCE/来源分区首块摘要绑定为不可变 `SourceSnapshot`；通过同一 `TargetSession` 在取得写租约、重开后**重新比对**，保持原 WAL 路径。 | 模拟同容量换盘、device number 复用、LBA7/LBA12/FS 首块篡改、拔插，全部写前拒绝；不突破已存在几何/协议块复核。注意当前属于待证风险，**不能先宣称已存在误写**。 | 未开始 |
| **P1-1** | **A04 + A06 + A08** | 将 `provision plan --source-backup`、旧 `native_preflight`/Mode1 专用写集与 TUI `NativeReadOnlyPlan` 迁入通用 `prepare_native_provision_on_disk → NativeWritePlan`；来源证据只作为校验输入，不能另造业务规划。把 CLI/TUI 必要格式化/保留、来源影响、能力矩阵下沉到 Application 统一事实，移除各处旧 512/4096 业务白名单。 | Plain/Mode0/1/2/3 × 四标准规格的计划事实/写集合合同源，保留区 SHA256/原 FileKey 不变；旧 CLI/TUI 功能/离线证据不回退，已无正式代码消费者后再删旧 API。 | 未开始；统一正式入口已具备 |
| **P1-2** | **A03** | v3 512B 元数据恢复保持可用；对 v4 1024/2048/4096B 先完成**只读恢复计划**、同身份同几何验证、备份完整性/源 extent、LCE 原生块尾及 WAL 写集合合同。独立验证通过再新建有版本的 restorable 契约与写入执行器；跨逻辑扇区大小原样恢复拒绝。 | 512/1024/2048/4096B：备份→校验→只读计划→仅在批准的虚拟块设备 WAL 恢复→独立重挂识别、哈希匹配及失败回滚；未达到不得改变 v4 **evidence-only** 标识。 | v4 取证和只读核验已完成；恢复未实现 |
| **P1-3** | **A07** | 补无损/部分保留/仅改密/格式化边界的真实 OS 虚拟块设备验收；对来源保留的每个实际分区**全范围**和 LCE 完整 native extent 比较 SHA256，不只读 MBR 可枚举启动区。 | 四规格按 OS 实际支持级别分别标记认证状态；变更分区起点或容量时**强制格式化重建或拒绝**；错误密码不可被解释为用户允许数据丢弃；对 1024B 首轮中断保留未明原因。 | 破坏性 100/100 有历史证据，无损真实写入未闭合 |
| **P2-1** | **A09 + A10** | WAL 使用独立 `TransactionJournalReport`、可恢复 EDPB 使用 `MetadataBackupReport`、格式化状态独立；确认 UI 结果不把 WAL 称为用户数据备份。盘点旧 `PreparedProvision::Official/Plain`、`prepare_provision_on_disk` 消费者后逐步移除正式产品旁路。 | CLI/TUI 结果对应真实 WAL/EDPB 事务语义、恢复按钮能力正确；静态编译和应用行为回归通过。 | 待办 |
| **P2-2** | **A11 + A12 + A13 + A14** | 修正文档旧的“所有转换默认破坏性重制”和 CLI 镜像文案；`--include-virtual` 全局 AtomicBool 长期改为设备发现策略对象；原厂 FileKey 生成/复用依据实盘记录做金标，不以推测重设密钥；216 冗余候选逐项鉴别，不删除历史协议证据。 | `scripts/audit-redundancy.py --check` 0 确认缺陷；不改变既有用户数据、默认密码/加密语义；文档、帮助、CLI/TUI 真实行为一致。 | 待办 |

**开发约束／停止条件**：

1. 任何来源 `start_lba`、容量、加密/文件系统不兼容，均须**格式化重建并在用户确认中明确显示来源分区名**；未获丢弃授权就拒绝。**禁止**引入数据搬移、缩放、重新分区后的无损复制或 `DataMigration`。同构保留、已验证 FileKey 仅 rewrap 不得修改原密文、LCE 或未归属原生块尾部；所有写集合合同源确认摘要。
2. CLI/TUI、USB/Disk Image、512B/1024B/2048B/4096B 仅有一条 `SourceSnapshot → TargetProvisionPlan → NativeWritePlan → TargetSession → WAL → Readback` 业务路径；禁止独立 Mode1/4Kn 制盘写入器。`--include-virtual` 只控制发现，不授予额外写权限。
3. **实体 USB 未有本轮单独明确授权时禁止写盘**；只用已确认归属且 `VirtualOrPhysical=Virtual`、`BusProtocol=Disk Image`、原生块/容量均经核验的临时虚拟设备做写入测试；不绕过执行环境拦截。严禁 `git reset/clean`，禁止覆盖他人工作；依赖核对后小步提交并 `push`。
4. 按阶段先针对性单测 + `cargo fmt --all`/`git diff --check`，跨模块收口用 `scripts/test-fast.sh`；重大改造最后运行 `uv run --locked python scripts/test-full.py --profile full` 与冗余审计。**GitHub CI 仅在最终合并 main 阶段核查**；不要每次修改就重复 100 次破坏性矩阵。
5. **验收必须可复核**：记录来源/目标模式与几何、分区唯一 ID、计划影响、实际 WAL 写集、受影响完整 LBA 范围/哈希、独立重挂识别及结果；备份取证、只读规划、正式 WAL 写入、实体盘验收四个证据层级不能混称。

**下一 AI 接手起点**：先核对并收口其他 AI 的 A01/A05 未提交代码（有用就留、测试后分别提交推送），然后处理 P0-2 来源身份绑定；完成后按 P1-1→P1-2→P1-3→P2 执行。每阶段把准确证据追加至本计划，杜绝只汇报规划不改代码。

---

日期：2026-10-10。基线：`feat/native-4kn-wal-staging-20261010` / `eb1b9077`。本文件是下一迭代的**执行与验收计划**，不是已完成报告；历史实验与阶段证据见 [原生制盘现有验收计划](NATIVE_PROVISION_FINAL_ACCEPTANCE_PLAN_2026-10-10.md)、[多逻辑扇区架构](../../docs/architecture/MULTI_LOGICAL_SECTOR_SUPPORT_2026-10-09.md)、[官方启动区金标](../protocol/OFFICIAL_BOOT_SECTOR_GEOMETRY_AND_FAT_2026-10-10.md)。本计划在来源保留、备份恢复、全工作流方面扩展此前“允许丢弃数据”的破坏性验收目标；不得将较早的破坏性验收成果当作无损保留证据。

## 2026-10-10 最新需求修订（对下列旧表述具有优先级）

本计划的 **来源数据丢弃/保留、25 模式转换、分区起点/容量变化** 执行规则已由 [统一来源影响及确认页收口计划](SOURCE_TARGET_IMPACT_UNIFIED_PLAN_2026-10-10.md) 固化。**任何容量增加/减少或分区起点移动都强制格式化重建；本迭代不实现原位扩容缩容、文件级迁移、数据搬迁及文件系统拆分/合并。** 本文件旧段落里“DataMigration/流式数据暂存/完整字节迁移/移动缩放数据”等均视为过时的条件性设想，不是待开发项目或执行许可。用户选择保留时不兼容必须停止；只有明确同意丢弃来源数据时才能进行破坏性重建。

确认摘要必须按来源分区唯一 ID 输出**“来源数据丢弃：<分区名>”“来源数据保留：<分区名>”**，而非按目标分区统计“几个区域将清空”；目标格式化及密钥操作另以分区名列出。拆分一个二合一区到启动区、交换区，来源受影响仍只有**二合一区**。与此冲突的旧 S1/S3/S4/S5 迁移表述均以上述新计划为准。

## 2026-10-10 19:20 实施状态及下一步执行顺序（高于下文旧阶段推断）

**当前基线**：`feat/native-4kn-wal-staging-20261010` / `4efbc7ab`，本地与远端一致、工作区干净。已有 `53c35b32` Application 唯一 SourcePartitionId→SourceImpact 投影和 CLI/TUI 确认页同源；验收报告见 [四规格 OS 虚拟设备端到端验收](MULTISECTOR_VIRTUAL_OS_E2E_ACCEPTANCE_2026-10-10.md)。

**已取得证据**：512B、1024B、2048B、4096B 各 25 个有序 Plain/Mode0–3 来源→目标的**破坏性重制**，100/100 不同组合已在实际 macOS Disk Image 原生块设备上完成正式 CLI WAL 写入、重挂、模式识别；1024B 首轮 22/25 后一次原因不明中断，隔离最后 3/3 通过，不得声称连续 25/25 稳定。四规格 Plain exFAT OS 挂载写文件/重新挂载回读，Mode0 LCE/密钥/加密 FAT 独立只读校验和自定义密码解封装均通过。8192B 已在只读规划阶段正确拒绝 exFAT。Fast 复跑 8 套/10 产物/0 失败、29.24 秒；首次冷启动测试功能零失败但 64.46 秒超时。**此证据均不等于来源数据无损保留、备份恢复、TUI 全流程或实体 USB 验收。**

**即刻执行，按依赖顺序、小步提交和针对性虚拟盘验证**：

1. **S1-A 统一默认保留**：CLI/TUI 均默认保留每个几何/FS/加密语义兼容的来源分区；只有新建/不兼容的目标分区自动选中必要格式化，仍须在确认页告知来源数据丢弃，且必须明确确认后才写盘。CLI `--preserve-unformatted` 是额外的**严格无自动丢弃**模式：未显式 `--format-*` 的不兼容分区一律拒绝；不能当作默认保留的开启开关。原 CLI 自动全格式化为已确认的行为缺陷，禁止继续兼容该缺陷。目标密码缺省代表不要求改密，**不是**重置成默认密码；新建格式化分区没有目标密码则使用 OEM 默认密码。用户请求保留却无法证明来源几何/FS/加密/FileKey 兼容则 fail closed；绝不静默格式化。
2. **S1-B 无损保留与仅改密虚拟 E2E**：512/1024/2048/4096B 至少在可复现来源样本上分别覆盖全保留、部分保留、来源密码正确/错误、改密不格式化、容量/起点变化强制格式化或拒绝；同源影响摘要和实际 `NativeWritePlan` 一致、保留区原始 SHA256 完全不变、FileKey 同一值、WAL 写集合不得覆盖；先用既有实际 OS 虚拟盘正式 CLI，避免只做离线推演。范围变化一律重建，**不开发数据搬迁、缩扩容、流式 DataMigration**。
3. **S2 FS/协议补强**：Mode1/2/3 的独立逐区域 FileKey/LCE 解密、四种标准扇区 FAT12/16/32/exFAT 金标与错误边界、固定 512B 协议字段和原生 LBA 的类型区分；已通过的 Mode0/Plain 不重复无意义的完整矩阵。
4. **S3 EDPB 四规格备份与同几何恢复**：512B 继续使用已验证的 v3 元数据备份路径；1024/2048/4096B 先使用 v4 原生取证路径（**evidence-only，只读，不承诺恢复**），按每种真实原生块长度保存 LBA0–12、LCE 全跨度和分区首块。完成 v4 完整性/身份/预览和实际 OS 块设备采集测试后，再设计同几何 WAL 恢复并逐项解禁；跨逻辑扇区大小恢复仍拒绝。WAL 不等于用户数据备份。
5. **S4 TUI PTY 与 Inspect**：四规格实际运行页面输入/分区布局/确认摘要/进度/结果/ESC/重扫；Raw/Decode/Mixed、跳扇区、复制、设备容量与原生块尾部；与正式 CLI 同规划/写集合。
6. **S5 事务故障注入与观测边界**：WAL 创建、短写、LBA0 最后提交、断开与恢复、错误密码/容量/扇区尺寸变化；追踪 1024B 首轮中断的具体错误并保留诊断日志，不能由孤立重试通过推断已修复。
7. **S6 最终收口**：Fast/Full/冗余审计及整洁构建，专用获授权可擦写物理盘实测后明确各规格硬件证据级别，审计 PR/分支，合并 main 并核对版本/安装；**GitHub CI 只在最终合并 main 时检查**。

**2026-10-10 本轮实施回执（S1-A 已代码完成、S1-B 部分验收）**

- 已改 `src/application/provision/native_flow.rs`：移除 CLI “三项格式化均未选则全部格式化”逻辑；统一由 `TargetProvisionPlan` 来源几何/FS/密钥兼容判定驱动逐区自动重建；保留区实际写集合交叠仍 fail-closed。若来源密码不明却显式请求改密，按 Blocked 停止，不能因 Blocked 自动擦盘。
- 已改 CLI 参数→Request：目标密码缺省为 `None`（保持原密码包装）；新增 `--preserve-unformatted` 严格不允许自动格式化的显式选择；新建/重建时目标密码缺省仍采用官方 `0000aaaa`。原 CLI 默认整盘格式化的历史行为不再作为兼容承诺；帮助文案同步更正。
- 新增四规格 Mode1→Mode0 **无显式格式化开关**的定向测试：来源二合一区丢弃一次、目标启动区/交换区重建、来源保密区及原 FileKey/WAL 排除区保留；严格无自动丢弃选项阻断同一转换。已有四规格保留原密文/LCE/FileKey 与仅重新封装 FileKey 的源码单元测试仍通过。
- **回归结果**：`cargo test --locked --test provision_suite` 303/303；`cli_suite` 112/112；`tui_suite` 461/461；`--lib` 427/427（2 ignored），总计 1303 通过、0 失败、2 ignored；`cargo fmt --check` 与 `git diff --check` 通过。
- **正式 4Kn OS 虚拟盘验收**：经 `diskutil` `VirtualOrPhysical=Virtual`、`BusProtocol=Disk Image`、4096B、512MiB 和自有 image 文件绑定验证；CLI `provision plan` 对 Mode0→Mode0 输出来源全部保留/目标无格式化/无密钥操作，正式 `provision write` 使用 WAL 写入 13 原生块并成功回读。当前写前/写后 **MBR 可枚举的启动区** SHA256 一致；因为 MBR 仅枚举到启动区，不能冒充全分区哈希验收。
- **未完成**：实体 OS 虚拟盘的改密写入进一步测试遇工具安全拦截，已停止未绕过；其只读/离线 FileKey 复封装与全四规格源码验证已完成，仍必须补正式 WAL 密文哈希；512/1024/2048B 正式 OS 保留写入、数据区全范围 SHA256、错误密码与 WAL 故障注入、完整 EDPB/Inspect/TUI 测试继续按 S1-B→S5 执行。任何未获得独立证据的项目不标完成。

**2026-10-10 S3-a 原生取证备份阶段回执**：提交 `9fc54cc1` 已推送（P1/P2）。随后扩展 `src/application/write/backup.rs` 的 1024/2048/4096B 调度到 `native_backup::create_native_evidence_on_disk`，而不是错误地走 512B legacy 路径；`native_backup.rs` 使用观测的逻辑扇区大小解码原生分区、读取全 13 块、按 3072B LCE 的 3/2/1 原生块跨度捕获完整密文及物理尾部，并写入 EDPB v4 metadata-only/evidence-only。512B 与恢复逻辑未改，非标准 8192B 仍拒绝。`cargo check --all-targets` 通过、`backup_suite` 78/78 和 legacy guard 1/1 通过；已有 `tests/edpb.rs` 覆盖 1024/2048B v4 完整容器读回，4Kn 独立读取与禁止恢复合约验证。**尚未取得 1024/2048B 物理设备实际取证证据，也未实现 v4 同几何恢复。**

**2026-10-10 专用 4Kn 预检去重审计（后续优先收口）**：官方 Mode0–3 的正式 `provision plan/write` 已采用统一的 `prepare_native_provision_on_disk`，原有 `provision plan --source-backup` 却会选用早期 `native_preflight.rs::prepare_native_mode1_on_disk`（强制 4096B、Mode0→Mode1、另行生成写集），其 TUI 旧异步任务 `request_native_mode1_readonly_plan` 亦沿用历史 `Native4knReadOnlyPreflight` 数据结构。**该额外业务路径没有当前架构必要性，必须后续迁入统一规划/预检并删除历史特殊写集，不能将其作为正式制盘的另一套实现，也不能因为改名就视为已清理。** 本次仅完成通用 EDPB v4 只读来源匹配的 1024/2048/4096B 共用验证器 `verify_native_backup_against_disk_readonly`，删除同义 4Kn 包装函数；历史 Mode1 路径在自身上层仍保持 4096B 门禁，防止在迁移完全验收前误扩大特殊写集的使用范围。3 规格字节尾部篡改/短块拒绝测试通过；Provision/CLI 全套目标测试和备份测试均通过。下一实施项：把旧 `--source-backup` 入口或 TUI 回调的用途收敛为通用“来源身份与快照只读比对”，涉及目标分区的 `plan` 一律复用 `NativeWritePlan`，删除 Mode1 专用逻辑前证明没有现存消费者依赖；仅离线镜像的兼容行为另行隔离，避免未经验证的写路径。

**阶段停止条件**：不得将“不格式化但未证明保留”写到设备；待保留区任何原始块变化、FileKey 不可验证、大小/偏移/FS/加密状态变化或来源身份不可信时明确拒绝。任何测试专用虚拟盘均须核查 `diskutil` 的 `VirtualOrPhysical=Virtual`、`BusProtocol=Disk Image`、容量和原生块大小及镜像归属，未授权实体盘完全不写。其他 AI 未提交文件先审计，不 reset/clean。测试日志附项目进度记录。

## 一、最终交付定义

1. **一条业务路径**：CLI、TUI、物理 USB、OS Disk Image 使用同一 `prepare_native_provision` → 不可变 `NativeWritePlan` → `TargetSession` 身份锁定/租约 → 原生 WAL → 写入/同步/回读/状态恢复。`--include-virtual` 仅改变发现范围，不决定能否写、如何加密、如何规划。不得为了 HIL 新建特殊制盘器。
2. **一条设备几何真相源**：每次打开设备产生已验证 `DeviceGeometry`，原生 `LBA` 始终表示 `LBA * logical_sector_bytes` 字节。EDP 协议已确认字段按原生块内前 512B 解码，既不能乘/除 8 重新解释 LBA，也不能拿 6656B 的解析投影当作 4Kn 13 块备份。设备物理扇区大小单独记录，支持 512e 对齐与报告。设备逻辑块大小是观测值，软件不修改硬件报告值；不设置可变的进程级全局扇区大小。
3. **完整目标模式**：Plain、Mode0（三分区）、Mode1（二合一）、Mode2（兼容保留区＋保密区）、Mode3（内外网双分区）任意来源→任意目标，共 25 对。每对允许明确的“破坏性重建”；同区几何/密码/文件系统满足条件时还应支持明确的“原样保留／验证保留／仅重新包裹 FileKey”。需要移动或改加密算法的源数据，不可冒充原样保留。
4. **全流程**：设备识别、Inspect Raw/Decode/Mixed、LCE、表单/计划、密码验证、文件系统格式化、元数据备份、恢复、WAL、结果页、拔插重扫共用几何。不能以只读预览/离线金标代替真实事务验收。
5. **协议保真与缺省策略**：官方 `GLOBAL/bootSize=10MiB` 是独立 type1 启动区的**绝对结束字节边界**，原生起点 LBA63；512B 默认 20417 块、1024B 10177、2048B 5057、4096B 2497。Mode1 不另造 type1；Mode2 官方特殊兼容保留区按自身规则，不能套用 10MiB。Windows OEM `FormatEx("FAT")` 没有硬编码 FAT16；依据实际 FAT 簇数求解选择 FAT12/FAT16。其他扇区规格只能在证据范围内推导，不能宣称已复刻所有 OEM BPB 字节。

## 二、逻辑扇区能力分级（必须由一个能力注册表生成 UI/CLI/测试预期）

| 逻辑扇区大小 | Native I/O / WAL / 协议读写 | FAT12/16/32 与 exFAT 新建 | 目标验收级别 |
| --- | --- | --- | --- |
| 512B | 已有完整基础 | 标准规格，按容量与驱动规则允许 | 正式 CLI/TUI、备份恢复、物理 USB、OS HIL |
| 1024B | 已有离线 I/O/协议样例 | 标准规格，需逐 FS 容量/簇数金标 | 全业务源代码与离线端到端；获取真实 OS 块设备后再做 OS HIL |
| 2048B | 已有离线 I/O/协议样例 | 同上 | 同上 |
| 4096B | 已有 OS Disk Image 正式 CLI 写入证据 | 标准规格；10MiB 边界实例为 FAT12 | 正式 CLI/TUI、备份恢复、OS HIL；4Kn 真实 USB 尚需设备 |
| 其他 512B 整数倍，如 1536/2560/3072/8192B | 已有部分离线原生块 I/O、协议、WAL | **禁止伪造不合法 FAT/exFAT**；按 FS 标准拒绝 | 支持探测、Inspect、原生数据审计/事务模拟；在格式化能力不足时输出准确阻断原因 |

不将 1024/2048B 的离线验证冒称 macOS 原生块设备验收；不将 4Kn OS Disk Image 冒称 USB 控制器实机验收。NTFS 仅维持既有识别/读取能力，本迭代不以新建 NTFS 作为 FAT/exFAT 完成条件。明确区分 MBR 32-bit LBA、FAT 卷大小、操作系统挂载限制和原生 I/O 能力四套边界。

## 三、按依赖顺序执行（阶段完成即留证据、提交与推送）

### S0：现状冻结、单一契约与误写门禁（最高优先级）

- 核实 `git status/HEAD/branch/remote`、既有 PR/其他 AI 正在修改的文件；不 reset/clean、不建立冗余 worktree。保留已证实的 512B/4096B 14/14 OS 原生块 CLI 验收、官方 FAT12/FAT16 金标和本轮真实 U335 用户反馈。记录这些是“破坏性重新制盘”而非无损转换。
- 建立统一 `NativeSectorCapability`/`DeviceGeometry` 及 checked LBA↔字节转换入口；分离 Device I/O、协议 512B 投影、MBR 条目、FilesystemFormat、EDPBBackup、EDPBRestore、OS/HW Certification 等能力，避免不同模块各自假设 512B/4Kn。所有 512B 常量按“协议合法/磁盘 I/O 错误假设”分类，不机械替换协议字段长度。
- 保持提交 `eb1b9077` 已修复的 **preserve fail-closed**。不可再把 `format: false` 解释成“所有格式化标志为 false 所以全部强制格式化”；未确定数据/密钥保留能力时，必须在 `prepare` 入口停止。
- **门槛**：512B/4096B 行为无回退，所有制盘入口选择同一共享规划器，读取/预览不触发写入；最小一组状态与能力注册表合约测试通过。

### S1：真正的来源感知保留/改密（当前首要阻断）

- 把旧 `src/application/provision/prepare.rs` 已具备的 `PreserveOpaque`、`PreserveVerified`、`RewrapVerified` 判定迁到统一 `src/application/provision/native_flow.rs` 的 `SourceSnapshot→TargetPlan→WriteSet`，TUI 预检查与正式 prepare 使用同一个最终结论，禁止两个规划器分别给出“可保留/必须格式化”。
- 不变条件：原分区 `start_lba/count/native_sector_bytes`、分区类型、实际文件系统、物理加密开关与加密算法满足兼容；保存原数据区、LBA7/LBA12 与 FileKey 记录；原样透传时**不生成新 FileKey、不写 FAT/exFAT 引导元数据、不重置加密区**。Opaque 保留只接受已证实的严格同构情形；否则需要正确来源密码、实际解密与重新核验。
- 改密不格式化时，先以真实来源密码验证 unwrap 和 CRC，只重新包装同一 FileKey，保持物理密文扇区及其变换语义不变。变更分区起点、大小、算法、加密状态或重叠范围时不得标为无损原样保留；要么建立有独立源快照、完整字节迁移、重加密、回滚能力的 **DataMigration**，要么在 UI 明确指出不可无损保留，并要求用户显式选用破坏性重建。
- 模式变化和同模式重制都按分区独立处理“保留/迁移/重建/移除”，包括 Mode0→Mode1、Mode1→Mode1、Mode1→Mode0 等。不要为了让 Mode1 保密区“可保留”，静默清空其 FileKey 或重格式化。
- **门槛**：512B/4096B 的 Mode1→Mode1 全保留、Mode0→Mode1 保密区透传、同模式保留、已验证改密不格式化，以及密码错/未知、算法不匹配、范围重叠的拒绝例；目标文件/密文哈希及原 FileKey 可独立验证，未修改扇区原始字节不变。通过后才能移除“保留·待支持”。

### S2：模式布局、官方启动区与文件系统统一

- `OfficialPartitionMode`/Plain 全部走同一字节→原生 LBA 求解器；区分固定 LBA63、10MiB 结束边界、Mode2 特殊 `0x7E00` 字节区及 LCE 起点/长度。任何 byte/sector 换算必须 `checked_{add,mul,div}` 并定义 ceil/floor/自然对齐规则。1024/2048B 的官方模式逐项验证协议可表达性，不以仅有的几何推导冒充 OEM 实测。
- FAT12/16/32/exFAT 用同一 FS 几何契约，依据 `sector_bytes`/分区范围/BPB cluster count 选算法；禁止根据 `sector_bytes == 4096` 硬编码 FAT12、或无意提高簇数绕过 FAT 类型验证。校验 FAT、FSInfo、保留扇区、卷标、exFAT checksum/bitmap/Upcase 表、主备引导区及文件元数据真实字节；MBR 文件系统类型/起点/长度按 Native LBA 生成。
- LCE 内容固定 3072B 以真实原生块跨度向上取整，加密地址使用物理字节偏移；4Kn 多出 1024B 加密零区依据已知证据生成；1024/2048B 及非常规几何必须有原始块回读、密钥、尾部不明区保留策略，不得复刻官方 Windows 工具越界缓冲行为。
- **门槛**：每种标准规格的每个可格式化分区类型都有独立格式化和解析金标；启动区 512/1024/2048/4096B 几何正/负例，通过兼容/CRC/分区解密检查；512B 旧盘金标完全无回退。

### S3：备份、恢复、Inspect 与所有 UI 一致性

- 统一 EDPB 备份与恢复的 native block/byte length、extents、身份/容量/逻辑扇区约束。**当前 `src/edpb/write.rs` 仅认 512/4096B，并将 4Kn 作为只读取证**：补齐合法标准规格的元数据捕获、完整原生协议 13 块、LCE 与经证实的尾部，重新设计可恢复策略；旧 EDPB 版本只读兼容不得破坏。WAL 是事务前镜像，不是用户数据全盘备份。
- 同逻辑扇区尺寸的恢复可依据备份完整性/介质身份/相同区域执行；跨逻辑扇区大小恢复必须作为显式转换新计划，不可把旧 LBA 原样解释成新 LBA。大规模移动/缩放数据需完整来源数据副本和受控迁移，不能只靠 13 块备份声称可无损转移。
- Inspect 逐原生块显示 Raw、Decode、Mixed；协议字段的固定 512B 视图与原生块尾部可辨；4Kn LBA11 OpaqueTail 保留、默认隐藏敏感原始字节。扫描、LBA Jump、表格、容量地图、原始/解析对比、结果页和复制均以同一几何/解码规则工作。
- CLI 和 TUI 同 request 产出同一目标分区、密钥语义、文件系统、原生块写集；UI 必须显示来源/目标保留状态、哪些数据会丢失、密码需要验证的原因、WAL/EDPB 的真实性质。
- **门槛**：512/1024/2048/4096B 元数据备份→校验→预览→同几何恢复→重新独立识别；旧备份兼容；跨几何误恢复拒绝；Raw/Decode/Mixed 和 TUI 结果页完整滚动及真实容量一致。

### S4：原生事务、WAL 与数据迁移的可靠性

- `TargetSession` 获取设备当前几何、打开句柄/身份、写租约；USB 与 Disk Image 不分叉。实际写集合并同一 LBA 的不同所有权字段，禁止部分块写入和无意改动 `OpaqueTail`；保留区做 write-set disjointness/hash 审计，LBA0 最后提交。分区移动/全量数据复制不能只靠小型 WAL 声称可回滚：另设流式数据暂存/逐范围校验、可恢复检查点与空间门槛。
- 故障注入覆盖：snapshot/WAL 创建及持久化、磁盘写入中断、LBA0 前后、sync、readback、装置断开、权限拒绝、WAL 损坏/重复恢复、来源被替换、logical sector size 漂移、设备号复用、容量变化；必须产生可复核 `Committed/RolledBack/MediaIntermediate/MediaStateUnknown`，有界停止，不吞掉失败。
- **门槛**：标准 4 规格离线原生 WAL 故障矩阵全通过；512B/4Kn OS Disk Image 独立故障回读或回滚；来源保留范围在成功/失败案例中均无意外变化。

### S5：机器可复核的完整模式与几何验收矩阵

- **最低 100 对模式转换**：四个 FAT/exFAT 标准扇区规格 × 五种来源 × 五种目标；每对执行“创建来源→重新打开独立识别→正式制目标→重新打开独立识别→解析 FileKey/LCE/FS”。输入种子每次独立，结果以表格 + 哈希留存；既有 8 规格×25 的离线“破坏性写集”仅作为历史基线。
- 512B/4096B 分别运行 25/25 个**真实 OS 块设备**组合；若系统未提供 1024/2048B 的可验证块设备，采用真实稀疏文件 I/O/原生 WAL 全流程，OS HIL 列保留“未认证”，不伪报通过。单独覆盖 512e 的逻辑512/物理4096 对齐（当有可观测设备）。
- 对每种可保留布局执行全保留/一部分保留/改密/全部重建四种操作契约；跨模式数据移动如尚无已验证迁移引擎，只能明确拒绝无损并允许经显式确认的重建，不能宣称全场景“无损”。
- 真实 TUI PTY：选择设备、格式化勾选状态、编辑/预览/确认、实际写入、进度、容量地图、分区最终状态、Esc 返回、重扫；与 CLI 同参数规划事实一一比对。文件系统挂载需在实际 OS 支持的规格验证创建文件、sha256、卸载重挂，再验证；密文需独立解密而非宣称 OS 可直接挂载。
- **门槛**：自动导出 `sector_bytes × source × target × outcome × level × readback` 机器结果；无证据的组合必须显示阻塞而非成功。真实 USB 需用户指定测试盘并授权，当前 U335 禁止破坏性测试。

### S6：发布、安装与持续门禁

- 各阶段只做**针对性测试**并小步提交/push；跨模块接口冻结或阶段收口时才统一执行 `cargo fmt --all -- --check`、`scripts/test-fast.sh`、`uv run --locked python scripts/test-full.py --profile full` 和冗余审计；OS HIL 在 S5 阶段统一跑，避免每修一次展示字段就重跑 14 次写盘。
- GitHub PR/分支/worktree 治理先审计再处理，不 reset/clean。完成后合并 main 并核对 origin/main，升级与项目现有版本号策略一致的版本，构建安装后核对 `~/.local/bin/edpcli` 的实际版本和 SHA。
- 固化能力注册表及协议字段分离的测试门禁：任何新代码把原生 LBA 错乘 512、任何“保留→自动格式化”、任何混淆 WAL/EDPB、任何 MBR 类型和 FAT 真实簇数不一致均拒绝合并。
- **门槛**：标准几何全功能实际通过；非常规格按分级契约行为正确；Fast/Full/审计/OS HIL 日志可追溯；缺少实体 4Kn、1024B、2048B 的硬件实机证据单独列为待验收，不能冒充最终通过。

## 四、先做什么、什么时候停止

**推荐优先顺序：S0（统一契约）→ S1（无损保留）→ S2（FS和协议）→ S3（备份恢复/Inspect）→ S4（WAL）→ S5（矩阵与实机）→ S6（发布）。** S1 是用户已经碰到的真实阻断，不允许推迟到发布最后。S2 与 S3 的纯只读测试可在 S1 的独立文件中准备，但绝不能先宣称 S1 完成。

任何一项出现来源身份不明、实际容量不匹配、未验证 FileKey、原生跨度与分区表冲突、用户期望保留但写集会覆盖旧区、备份不能容纳拟迁移数据时，停止写盘并提供准确的拒绝原因。用户显式选择“允许全部清空”时仍应允许有证据支撑的完整重建，而不是把不支持的无损保留误变成强制格式化。

## 五、验收报告必须包含

- 基线/目标/分支/提交与运行 `edpcli version`；实际工作区状态及执行日志；
- 四类标准扇区规格的模式矩阵（100 对）及 512B/4Kn 的 OS 设备完整 25×2 矩阵；
- 每种格式的 BPB/FAT/exFAT 信息、MBR/EDP协议、LCE 的原生范围和 CRC/独立解密证据；
- 至少一个保留密文完全不变、一个仅改密不格式化、一个因移动重叠拒绝保留的可复现样本；
- 备份/恢复原生范围和新旧 EDPB schema 兼容证明；
- WAL 故障注入、独立回读与有限故障停止证据；
- 真正运行的 CLI/TUI 同源计划、操作回放、文件系统实际写文件/重挂哈希；
- 操作系统/真实 USB 硬件缺口及哪些功能仍需外部设备，明确**未验收不等于已通过**。
