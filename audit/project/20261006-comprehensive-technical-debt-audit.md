# 全仓库技术债与优化空间审计 · 2026-10-06

审计基线：`d6f93fc4aa43fab29306e90ef51a0f9f39bc96ad`，版本 `2.5.0`。本报告是当前统一待办，合并上一轮 F1～F8，新增 F9～F22；旧报告保留为对应提交的历史记录。本次只提交审计与复现材料，不修改生产逻辑、不安装二进制、不访问物理盘、不重新转换本地备份。

## 结论与优先级

共 **22 项已确认的优化工作：3 项 P1、16 项 P2、3 项 P3**。P1 是应先解决的写入契约或验证覆盖问题；P2 是明确存在的维护债、行为偏差或可靠性缺口；P3 是工程收敛。没有证据支持宣称出现了实际物理盘损坏或正在被利用的漏洞。

最优先的新增问题是 F9：容器声明“仅元数据”，校验和恢复计划却没有完整约束所有 Restorable 工件的语义与范围。SHA 校验、正确介质身份、目标容量检查都不能替代这项业务约束。其次是 F10：同一版本写入器能成功生成读取器拒绝的备份。

| ID | 优先级 | 问题与影响 | 主要责任位置 | 工作类型 |
| --- | --- | --- | --- | --- |
| F1 | P1 | GPT 重复读取绕过首轮预算；验证与归档字节分离 | partition_table / backup_metadata | 合并实现 |
| F2 | P1 | CI 路由遗漏协议夹具、EDPB、build.rs、发布工作流 | change_scope / workflows | 补验证边界 |
| F3 | P2 | 原文序列号身份不能可靠生成提权 resume pin | media_identity | 修复构造契约 |
| F4 | P2 | 14 个无消费者定义及无消费者文件载荷投影链 | 见删除清单 | 删除开发遗留 |
| F5 | P2 | 区域动作双状态、全盘旧文件系统字段并存 | provision/reprovision / layout | 收敛模型 |
| F6 | P2 | v3 必需字段仍为 Option | edpb/model | 收紧类型 |
| F7 | P2 | Inspect 无上下文旧入口仍猜测跨扇区状态 | inspect/lba_* | 删除开发回退 |
| F8 | P2 | SectorDev 同步、重开默认成功 | ports / diskio | 显式能力契约 |
| F9 | P1 | MetadataOnly 未限制全部恢复范围及完整性 | edpb/validate / write/restore_plan | 收紧恢复授权 |
| F10 | P2 | EDPB 写入端未遵守读取端预算 | edpb/write / limits | 统一资源预算 |
| F11 | P2 | EDPB 直接发布到最终路径；持久化契约分散 | edpb/write / backup_store / platform | 收敛文件生命周期 |
| F12 | P2 | lineage 不可覆盖依赖 exists + rename | provision/identity_lineage | 排他发布 |
| F13 | P2 | Secret 包装外仍有未清理临时副本与删除尾部 | cli/prompter / tui 输入 / cli_args | 收敛秘密生命周期 |
| F14 | P2 | 备份与 lineage 文件权限、提权后的所有者未统一 | edpb/write / identity_lineage | 收紧宿主文件权限 |
| F15 | P2 | 必需协议夹具缺失时部分测试静默通过 | tests/common / crypto / sectors | 修正测试契约 |
| F16 | P2 | 镜像与 Inspect 导出先覆盖文件，失败可留下残缺结果 | application/*/export | 原子输出 |
| F17 | P2 | 备份时间仍以文件名、mtime 决定排序和 prune | backup_store/catalog | 消除旧时间权威 |
| F18 | P2 | info 旧目录包装把扫描错误转成空备份列表 | backup_store/create / metainfo_cli | 传播类型化失败 |
| F19 | P3 | 构建元信息、工具链与发布元数据存在多份事实源 | build.rs / release / SBOM | 收敛构建事实 |
| F20 | P3 | 本地安装先替换，再验证，失败不恢复旧版本 | install-local.sh | 原子安装 |
| F21 | P3 | 公共命令目录与真实密码选项不一致 | command_spec / cli_args | 收敛语法描述 |
| F22 | P2 | 发布所需 CI/HIL 结果未在发布 DAG 中绑定同一 SHA | release / virtual-disk-hil | 固化发布门禁 |

## 审计覆盖与边界

基线清点 **741 个跟踪文件**：402 个生产树 Rust 文件（91,582 行，含内嵌单测），112 个测试 Rust 文件（47,372 行），5 个 Rust 示例，44 个脚本目录文件，6 个 GitHub 工作流/配置，15 个 docs 文件，79 个 audit 文件，23 个仓库 EDPB。全树执行调用名称、历史兼容、公开定义、unsafe、文件生命周期与前端依赖扫描，再核查各子系统关键调用链。行数用于范围说明，不作为拆分理由或生产代码净行数。

| 领域 | 本次核查内容 | 结果与优化归属 |
| --- | --- | --- |
| 根模块与架构边界 | lib 根导出、application 门面、领域副作用、AST 导入门禁 | 既有边界有效；F4/F5/F8，后续 API 收窄见低优先级候选 |
| CLI、语法与补全 | 实际解析、帮助、目录、密码输入、文件目标、提权参数 | F13/F18/F21；保留严格重复参数与整数范围拒绝 |
| TUI 状态与任务 | generation/session、单任务调度、关键 worker、结果应用、终端 RAII、渲染节流 | F4/F7/F13；已存在进度合并、退出等待与恢复后隔离，不重复列旧修复 |
| 介质身份与提权 | raw serial、pin/resume、重开复核、身份匹配与宿主 lineage | F3/F12/F14；保留当前摘要 pin 与原文身份区分 |
| 写盘事务 | 同步预检、touched-sector mirror、顺序、读回、三次回滚、observer panic 隔离 | F8/F9；未发现可绕过现有写前身份/几何检查的新调用链 |
| 制盘与格式化 | 官方 mode0～3、Plain、区域保留/重建、密钥域、稀疏镜像、逐分区提交 | F4/F5/F16；多阶段失败语义已明确，不要求伪造整盘原子性 |
| EDPB | v3 图、身份、帧/范围/SHA、读取预算、写入、恢复计划与预览 | F6/F9/F10/F11；旧 v1/v2 reader 没有重新出现 |
| 备份目录 | 展示缓存、exact-file 校验、排序、prune、隔离删除、路径配置 | F11/F14/F17/F18；删除的隔离与排他恢复机制继续保留 |
| MBR/EBR/GPT | 主/备表头、CRC、数组地址/大小、重复读取和归档 | F1；不把普通分区表重构扩大成 EDP 协议改写 |
| 文件系统 | FAT12/16/32 与 exFAT 分析、cluster/目录预算、环与交叉分配、格式化、NTFS 探测 | F4；现有 cluster、目录长度和路径预算有效，保留深验证 |
| 历史协议与密码学 | profile 注册表、LBA0～12、LCE/IIR、历史密钥、证据夹具与金标 | F4 仅无用别名/查询助手；F7/F15 不删除合法历史 profile |
| 三平台与 unsafe | Windows 缓冲长度/对齐与句柄、macOS IOKit RAII、Linux 挂载与系统盘拒绝 | F11/F14；本机不能替代 Windows/Linux 原生运行与实体 HIL |
| 子进程 | Unix 自有进程组、Windows kill-on-close Job、双管道排空、时间/输出预算 | 现有机制及相关测试通过；未确认新增缺陷 |
| 测试与夹具 | Cargo 显式 test roots、112 文件登记、非 HIL runner、超时/预算、真实夹具 | F2/F15；未发现未登记的 Rust 测试文件 |
| 依赖、构建与发布 | lock、deny、六架构、通用包、SBOM、安装、Git hook | F2/F19/F20/F22；依赖审计通过，重复版本仅列候选 |
| 文档与历史材料 | 当前架构/EDPB 文档对照旧报告，协议原始证据用途 | 本报告及索引统一当前待办；历史审计与协议证据继续留存 |

“全仓库审计完成”指上述范围的清点、扫描、关键链路核查与当前验证已闭合。它不等于每行代码的形式化证明，也不把未执行的其他平台原生测试、断电实验或实体盘 HIL 写成通过。

## F1～F8：既有发现的统一处置

F1～F8 的详细证据见 [上一轮报告](20261006-remaining-technical-debt-audit.md)，本轮复跑 F1/F3 有界复现，并扩大 F2/F4/F5 的范围。

- **F1：** 首轮 GPT 数组上限为 8 MiB，`validate_plain_gpt_mirror` 的第二轮没有相同上限；随后归档又读取一次。合并为包含主/备表头与数组原始字节的有界采集结果，归档直接使用已验证字节。验收包括首轮超限、第二轮表头/数组变化、地址溢出、CRC/几何冲突，且零写入。
- **F2：** `scripts/change_scope.py:8` 分类实测：协议 mode1 夹具 `rust=true, protocol=false`；`backup/example.edpb`、`build.rs`、`.github/workflows/release.yml` 三个开关全部 false。补全数据、构建入口和全部工作流路由；未知可执行输入保守选完整验证，并覆盖新增、删除、重命名及首次 push。协议夹具触发 protocol gate，EDPB 数据触发 backup gate，不能只跑 repository 文本契约。
- **F3：** `src/media_identity.rs:188` 从 snapshot 直接复制可缺省 serial 摘要；现行 raw-only v3 身份可以通过内存 pin，却生成不合法 resume pin。复用当前摘要生成规则。验收 raw-only、raw+digest、missing/suspicious 身份；不删除提权 pin，不恢复旧 EDPB digest-only 授权。
- **F4：** 按下文删除清单移除无消费者入口、过时测试期待和 locator 投影；保留 filesystem analysis 本体及空文件系统深验证。删除后验证公开导出、脚本/示例/测试消费者和全部非 HIL 套件。
- **F5：** `TargetPartitionPlan` 同存 `action` 和 `disposition`，多处手动同步；只保存 disposition，派生 preserves_extent。新增核查：`src/provision/layout.rs:144` 的 `filesystem_format` 只在构造时保存、在测试中读，运行时用 `filesystems`；移除旧字段，构造便利方法可直接生成逐分区值。验收模式/文件系统组合、保留区、格式化选项及稀疏导出；不修改 mode2 真实兼容区语义。
- **F6：** `Manifest` 的 backup_purpose、restore_contract、identity 已由 v3 校验强制 Some，改为必需字段，保持合法 JSON 形状。当前 Core 备份、无分区条目、缺真实硬件信息仍须合法；不借机开启新格式迁移。
- **F7：** `analyze_sector(..., None)` 触发 LBA2/LBA9 的猜测。迁移测试到完整上下文，删除无上下文旧包装和猜测；缺 companion sector 明确 MissingContext/Unknown 并保留 RAW。金标及所有真实历史 profile 结果保持一致。
- **F8：** `src/ports.rs:19` 的 sync/reopen 默认 Ok 掩盖实现遗漏；使实现显式声明，读写能力逐步统一已有 reader 接口。FileDev 目前实现了真实 sync/reopen，不能描述为现有原生写入都不持久。验收失败注入、锁生命周期、重开、同步与回滚；保留 u32 写入边界。

### F4 可删除清单

以下 **14 个定义**在 src/tests/examples 和相关脚本中没有调用消费者；重导出、定义本身和历史审计提及不算调用。所有删除都需同步收窄导出与测试，不能只添加 allow(dead_code)。

| 定义 | 位置 | 处置边界 |
| --- | --- | --- |
| PARTITION_PREFIX_SECTORS / PARTITION_SUFFIX_SECTORS | backup_metadata.rs:23 | 删除两项旧采集常量 |
| FORMAT_NAME | edpb/model.rs:4 | 保留实际使用的 EXTENSION |
| IDENTITY_HEADINGS | application/identity.rs:11 | 表格已使用现行 schema |
| summarize_backup | metainfo.rs:438 | 保留已验证 reader 的摘要入口 |
| write_core_backup_with_notes | edpb/write.rs:250 | 删除包装及 edpb 重导出 |
| request_provision_key_probe | tui/provision/task.rs:23 | 删除 session=0 入口，保留显式 session 方法 |
| request_provision_source_password_verify | tui/provision/task.rs:69 | 删除 String/session=0 兼容入口 |
| CapacityInput::to_quick | provision/reprovision/model.rs:98 | 保留现行容量编辑与显式取整规则 |
| ProgressEvent::with_stage / with_log_policy | application/progress.rs:336 | 删除两项无人使用的 fluent builder |
| InspectTopology::find_label_path | application/inspect_tree/model.rs:208 | 保留实际使用的 find_label_paths |
| IMAGE_BYTES | protocol/types.rs:5 | 仅删重复无用常量，保留 PROTOCOL_IMAGE_BYTES |
| profile_axis_detector | protocol/profile_detector.rs:581 | 仅删单项查询助手，保留整个注册表及 detect_profile_axes |

另有 `filesystem/analysis/mod.rs:102` 的 stream_file_payload 及 FilePayloadExtent/Locator/PayloadReadSummary、FileEntry.payload_locator 和 FAT/exFAT 构造链无业务消费者；可整链删除。它们不能成为删除 cluster 环、交叉分配、容量校验的理由。

`DiskFacts.label_id` 也是只写不读的冗余字段，可随模型清理删除；不能误删实际备份 onlyid 提取。`OfficialProvisionPlan.filesystem_format` 归 F5，避免重复计数。

**明确排除：** `backup_prepared_provision_on_disk` 虽然无当前直接调用，是强制备份 proof 的有效公共组合入口；保留或另行做 API 产品决策。原子写 API、协议历史算法、profile 注册表、稳定 application 门面及研究脚本不能仅因消费者少就删除。

## F9 · MetadataOnly 的实际写入能力超过声明范围

证据：`src/edpb/validate.rs:3` 检查契约布尔值、引用存在和部分原始长度；`src/application/write/restore_plan.rs:13` 遍历全部 Restorable，仅针对 LCE 和两种 tail 工件做名称特判。普通 raw_sectors 只要单 Extent、长度正确、目标范围内就可进入 transaction；未要求 completeness=Complete，也没有完整的元数据范围所有权证明。

临时容器复现：新增 role/purpose=`ordinary_file_payload` 的 Region/Extent，位置 LBA2048、1 sector，Artifact 标记 Restorable + Partial，数据 512B。写入及 VerifiedBackupReader 校验均成功。随后在审计程序中直接编译当前私有 restore_plan 源文件，保持生产可见性不变；生成的内存 transaction 包含 LBA2048 的 512B 写入。**没有打开设备、执行物理恢复或覆盖实际用户文件**。这证明校验器与实际计划构造都接受该范围；原生采集器通常不会生成这种工件，但容器边界仍应强制契约。

优化：引入类型化元数据工件/区域角色，在校验与恢复计划共享授权规则；EDP 仅允许经协议解析证明的 LBA0～12、LCE、合法 tail 元数据，Plain 仅允许经 MBR/EBR/GPT 证明的分区表范围。Complete、无重叠、source extent/region 包含关系与实际盘面应一致；未知/文件系统/用户文件范围只可 EvidenceOnly。不能仅依赖可改名的 artifact.id，也不能用唯一现代布局替代所有历史 EDP 布局。

验收：未知 restorable、Partial/NotCaptured、伪装 ID、错误区域角色/边界、重叠、文件区域被拒绝，所有拒绝发生在卸载/写入前，写入次数为零；合法历史 EDP、Plain MBR/EBR/GPT 与当前 v3 Core 备份保持可恢复。

## F10 · 同版本 EDPB 写入成功但无法读取

证据：`src/edpb/limits.rs:2` 限制容器 256 MiB、Manifest 4 MiB、单 Artifact 64 MiB、累计 payload 128 MiB、Artifact 1024 个。`src/edpb/write.rs:89` 在落盘前/后都未执行同一预算检查，validate_manifest_graph 也没有这些预算。

有界复现使用 1024 个 1B 扩展工件，加 raw protocol 共 1025 个：write_metadata_backup 成功，verify_file 报 artifact count 超预算。另一份约 4 MiB notes 导致 Manifest 超预算，写入成功、读取拒绝。未制造超大磁盘文件。

优化：共享读写 budget validator，写前计算输入数量、长度与溢出，序列化 Manifest 后检查其长度与预计最终大小；预算或图错误应在最终文件发布之前返回。验收每种上限的恰好边界及边界+1，成功生成的容器必须能由同版本读取器验证。

## F11 · 完整文件发布与持久化责任没有统一

证据：`src/edpb/write.rs:113` create_new 直接创建最终 .edpb，先写空 header，再写 payload、manifest/footer 和最终 header，完成后 sync_all。目录扫描能在写入期间看见不完整文件。返回 Err 会尝试删除，panic/进程退出不能保证该清理；这不意味着正常返回成功的文件一定损坏。

备份服务 create.rs:134/160 随后会同步目录，因此不能把 Unix 正常生产创建描述成完全没有目录同步。低层公共 writer 没有同样契约；Windows `sync_directory` 明确是默认成功，由系统缓存负责，注释还保留已取消的备份 sidecar 描述。

优化：同目录私有临时文件 → 预算/图校验 → file sync → 排他发布最终名 → 平台支持的持久化步骤。统一处理残留文件与发布后同步失败，明确“已发布但 durability 未确认”，不要把所有错误当作“未生成”。Windows 的能力保证应明确建模并单独验证，不能无证据宣称 Unix 目录 fsync 等价。

验收：并发扫描只见完整最终文件；短写、同步、发布、目录同步失败和进程中止有清晰结果；已有备份不覆盖，mandatory backup 仍经新鲜验证后才允许写盘。Windows 断电持久化需要平台实验，不在本次通过清单中。

## F12 · lineage 的不可覆盖是检查习惯，不是原子约束

证据：`src/application/provision/identity_lineage.rs:50/77` 两次 exists 检查后执行 fs::rename。Unix 的 rename 会替换在最后一次检查之后出现的同名文件。临时文件按该原语顺序复现了覆盖；没有为私有 persist 添加测试 hook，也没有宣称自然发生了 transaction_id 冲突。生产 ID 为 128-bit 随机数，正常独立操作碰撞概率很低。

优化：使用排他发布原语或平台 no-replace rename，已有实现中的 hard_link 排他恢复机制可作为参考；实现文件系统不支持时明确失败，不能降级为可覆盖 rename。验收相同 ID 并发发布仅一个成功、旧字节不变，以及发布失败、目录同步失败后的可恢复状态。

## F13 · 秘密容器没有覆盖全部输入副本

证据：`cli/prompter.rs:50` 使用普通 String，Backspace pop，Esc/Ctrl-C/读取错误先 clear，最终只 wipe 当前长度；删除掉的尾部不再被最终长度清理覆盖。`tui/restore_followup_state.rs:202` 先 truncate 临时 Vec，再 fill 当前长度。制盘 field_input.rs:143/168/185 对密码也建立普通 Vec<char>；cli_args.rs:104 保存四项密码为普通 String；现有自定义 Debug 已脱敏。

优化：在 SecretText/SecretBytes 内提供 push、删除和替换，擦除删除区间后再缩短；临时副本也由 secret guard 持有，非 volatile fill 不作为强零化保证。构造及后台请求用 owned secret 类型，避免经过裸 String/Vec<char>。对 CLI 密码参数提供交互或受控输入通道；已有 argv 传入无法靠 Debug 脱敏或事后 drop 追回，是否移除参数应作为公开 CLI 变更单独处理。

验收：取消、退格、多字节删除、验证排队被替换、后台失败、模式切换/退出均清理所有自己持有的秘密；日志/Debug/渲染不包含密码。不要更换历史 EDP 的密码折叠或加密算法。

## F14 · 宿主备份文件依赖 umask，提权所有者未治理

证据：EDPB 与 lineage 的 OpenOptions 未指定 Unix owner-only mode，create_dir_all 也未固定私有目录权限；临时备份在本机 umask 下为 **0644**。`platform/unix_support.rs:86` 已有 own_invoking_user_file，但 EDPB/lineage 创建链没有使用它。备份可能保存原始序列号、用户/部门和协议证据，因此应有明确文件权限契约。

优化：新建私有宿主状态目录与文件使用 0700/0600 的显式策略，并处理 sudo 创建文件归属；已有用户指定共享备份目录不擅自批量 chmod/chown。Windows 明确 ACL/继承策略并做原生验证。验收普通用户与提权后创建、失败清理、可再次读取/管理；不把本机 0644 观察推广为所有平台实测结果。

## F15 · 必需协议夹具缺失可以表现为测试成功

证据：`tests/common/mod.rs:44/57` 对已提交夹具的缺失/读错/长度不符返回 None，`tests/sectors_readonly.rs:25` 缺任一夹具直接 return；crypto_prims 的某些用例相同，sidecar 用例只要求 checked>0，缺两张仍可通过。当前金标完整，所以本次通过不是已证实的空跑；问题是将来的缺失不能由这些用例可靠发现。

优化：required fixture 返回 Result/expect；建立包含必需路径、长度、SHA 和覆盖 profile 的统一 registry，并要求完整集合。真正可选的实体 HIL 与研究样本另标 optional，不混淆。验收删一张、截断、丢 sidecar、未知 key 都失败；正常全套不跳过。保持既有 20 张 protocol baseline 的定义，新 mode1 夹具不强行混入不同用途的金标计数。

## F16 · 导出覆盖与失败发布缺乏统一语义

证据：`application/provision/export.rs:22/80` 对目标 create+truncate，随后 set_len 和写 sparse sectors；失败会留下残缺镜像并丢掉旧输出。`application/inspect/export.rs:44/46/58` 逐文件 fs::write，bin/hex/meta 可能来自不同完成阶段。用户选择输出路径可以表达覆盖意图，但不应让失败破坏此前完整结果。

优化：镜像写同目录临时文件并 sync 后替换；明确覆盖策略，避免跟随不预期的符号链接。Inspect 单次导出采用 run 目录或文件集合清单，成功后发布完成标记。验收中途 I/O/磁盘满/同步失败时旧完整输出保留、半成品可识别且可清理，成功导出的 bytes 与稀疏布局不变。

## F17 · 文件名继续控制“最新备份”与保留顺序

证据：`backup_store/catalog.rs:76/117/126` 以文件名 YYYYMMDDHHMMSS 优先、mtime 兜底；v3 已有 snapshot.created_epoch，但当前 BackupEntry 未保留为排序事实。prune_candidates 使用传入的 newest-first 列表。重命名或复制/touch 可能改变 keep=N 的保留对象；无时间文件名的 shell names 排序又与完整目录的 mtime 顺序不同。

优化：健康 v3 条目保留 validated created_epoch，以其排序；文件名用于显示名称，损坏文件只能显示观察时间/未知时间，不能自动 prune。轻量 shell 补全继续避免读取全部 payload，但不得声称其文件名次序就是完整 catalog 次序。验收 rename/touch/copy 不改变健康备份的保留顺序，时间相同稳定排序，损坏条目不进入自动删除。

## F18 · 目录错误仍被一个旧入口压成空列表

证据：`backup_store/catalog.rs:220` scan_backup_dir 失败时 eprintln 后返回 Vec::new；`create.rs:185` find_backups 使用该包装；`metainfo_cli.rs:277` 随后展示无本盘备份并返回 EXIT_OK。checked catalog 与 TUI 已能保存扫描失败，因此剩余包装会产生不同前端语义。

优化：find_backups 返回 Result 或共用 CatalogSnapshot 的明确失败结果，info 展示“关联备份不可确认”并选择一致退出策略；删除 print-and-empty 包装。验收不可读目录、条目/权重预算、读取错误与真实空目录可区分；只读失败不借机放宽写授权。

## F19 · 构建与发布事实源分散

证据：build.rs:64/65 假设 `.git` 是目录且当前引用是 loose ref；工作树/packed refs 下对应观察路径不成立。缺失 rerun 路径还可能使 Cargo 反复执行脚本，不能简单宣称必然得到陈旧 commit。dirty 查询覆盖全部跟踪文件，rerun 路径却主要覆盖 src/Cargo/.git；release 不设置 SOURCE_DATE_EPOCH，时间戳随构建时刻变化。工具链版本和执行器列表同时写在 rust-toolchain、多个 workflow、SBOM 与 release manifest 脚本中。

优化：用 Git 提供的 git-path/git-dir 定位元数据，定义 dirty 的实际覆盖范围；发布用提交时间提供 SOURCE_DATE_EPOCH；发布清单和 SBOM 读取固定配置/构建事实，减少硬编码。验收正常 clone、worktree、detached/tag、packed refs、源码包无 Git；同一源码/工具链的可复现性需比对实际产物，设置时间戳本身不等于已经可复现。

## F20 · 本地安装不是失败可恢复的替换

证据：install-local.sh:16 先 install 到 `~/.local/bin/edpcli`，随后比对 SHA、检查交互式 zsh、执行 --version；任何后续检查失败，脚本退出但目标已经替换。

优化：目标目录内生成候选、验证 hash/可执行性/version，确认交互式解析规则，再原子替换；发布后验证失败有明确旧版本恢复策略。保留现有用户目录、zsh -lic 与双 SHA 契约。验收损坏候选、解析到其他路径、版本命令失败和正在运行旧二进制；本轮不执行安装。

## F21 · 命令语法的事实源仍不完整

证据：command_spec.rs:129 的 PROVISION_COMMON_OPTIONS 未列四个 source/target password 参数，但 cli_args/provision.rs:226～247 接受它们，help.rs:57/60 又硬编码展示；tests/command_spec 主要检查几个指定选项和源码 contains，没有验证全部实际解析语法与 catalog 的集合一致。

优化：OptionSpec 增加敏感/是否补全/是否显示等属性，将所有公开选项登记到同一目录；无需自动补全密码值。语法、帮助与补全可分别投影，但应共享存在性和参数形状。验收公开选项全集一致、内部提权旗标明确隔离、已删除 CLI 参数持续拒绝。

## F22 · 发布规则没有全部绑定到自动发布任务

证据：docs/user/RELEASE.md:90 要求 main==origin/main、六架构 CI 与 Linux/Windows 虚拟 HIL 全绿。release.yml 的 publish.needs 只包含本工作流构建和 metadata；version job 核验标签版本，不查询同 SHA 的 CI/HIL 结果或 main 来源。虚拟 HIL 在独立工作流中。因而这些规则当前依赖发布前操作流程，不能由发布 YAML 的 DAG 单独保证。未检查远端 ruleset，不能宣称远端也完全没有保护。

优化：发布预检绑定准确 commit、来源与必需检查结论，或通过共享 workflow 显式依赖；缺失、pending、failed、被取消不能等价于绿色。明确把依赖审计、protocol baseline、虚拟 HIL 与架构构建结果纳入所需规则，按仓库发布政策定义 freshness。发布清单验证七套预期资产及各自 checksum，而不只对任意下载文件生成 hash 列表。

验收：不同 SHA 的绿色、缺失 HIL、未完成检查、版本不匹配、缺资产均拒绝发布；相同 SHA 完整通过后方可发布。虚拟 HIL 仍独立于日常 fast/full，不能为了此项把 ci-virtual-disk 混入普通测试门禁。

## 尚需测量或产品决策的低优先级候选

这些是优化空间，不计入 22 项已确认待办，也不编造性能收益或把缺测量写成缺陷。

| 候选 | 当前证据 | 推进条件 |
| --- | --- | --- |
| 合并重复间接依赖 | cargo-deny 提示 hashbrown 0.16/0.17、syn 2/3，来自 UI/宏依赖链 | 等兼容上游版本；先测编译时间/体积，不盲目 patch 不兼容 major |
| 收窄公共 Rust API 与重导出 | 未发布单 crate 仍公开多个实现模块，glob export 使无用定义难被编译器发现 | 先列消费者和保留接口；删除 F4 后按真正组合 API 收窄，暂不机械拆成 workspace |
| reader/sector 值类型统一与缓冲复用 | 已有多种 reader 和 Vec/[u8;512] 表达；事务会复制 patch/mirror | 与 F8 联动，先量化峰值内存、I/O 和分配；保持身份新鲜度与精确回滚 |
| CLI 机器可读输出 | 主要是人工文本，应用层已保留类型化结果 | 有自动化消费需求时设计稳定 JSON schema 和退出策略，不改现有脚本含义 |
| 源码字符串门禁逐步转行为/AST | architecture_split 有有效 AST 门禁，也有较多源码 contains 和行数预算 | 新边界用行为/compile-fail/AST 验证；保留证据契约，不为删测试而删测试 |
| 更强的格式/图关系变异测试 | 有短输入、CRC、范围和故障注入；F9/F10 说明组合契约仍有遗漏 | 先将确定复现变成回归，后引入有界属性/变异 corpus；不将长 fuzz 混入每次 fast |
| 大目录和 TUI 性能调优 | display cache、ReadControl、latest progress、20Hz render 已存在 | 使用 cold/warm、最大预算、慢磁盘实际测量；仅文件长或曾经卡顿不能证明需再次拆分 |
| 跨平台持久化/真实硬件矩阵 | 原生 CI 与独立虚拟 HIL 定义完整，实体证据按具体场景/平台记录 | 在有硬件时补证据；不能复制 macOS PASS 到 Windows/Linux，也不能删除现有 guard |

## 必须保留与可以移除的界线

**必须保留：** 历史 EDP 盘的 legacy-v0064、2019 transition、LBA6 历史 MBR underlay、LBA7/LCE 历史 wrapped key、LBA4 历史身份/密钥、LBA12 历史 EDPF，以及其原始证据、金标、模式编号和密码学向量。当前提权摘要 pin、介质/几何重开复核、mandatory backup proof、排他删除恢复、事务同步/读回/回滚、系统盘拒绝、只读取消预算与 HIL 功能隔离均不属于可删除兼容代码。

**可以移除：** F4 的已确认无人消费定义/投影、F5 冗余状态、F7 开发无上下文猜测、F18 print-and-empty 包装；删掉它们不要求保留开发时期 Rust API 别名。`legacy/compatibility` 名称本身不构成删除证据。

**EDPB：** v1/v2、legacy_migrated、旧 .bin 备份链和 digest-only 授权维持移除状态。现有离线转换与归档记录作为历史证据保留，不重新把迁移器接回运行时。现行 v3 中的 legacy_derived_candidate 是物理身份的派生证据，不是旧 reader，不能凭名称删除。

## 执行顺序与验收门禁

| 阶段 | 工作 | 依赖与验收 |
| --- | --- | --- |
| A：保护写入与验证覆盖 | F2 → F9 → F1 → F10 | 先修路由，再补确定复现的回归；所有契约失败零写入，合法历史协议仍通过 |
| B：可靠宿主文件与秘密 | F11/F12/F14/F16；F3/F13 | 共享排他/原子发布但不混淆不同覆盖策略；明确每一步失败状态，输入与提权行为一致 |
| C：删除开发技术债 | F4/F5/F6/F7/F8/F18 | 逐项删除消费者和旧测试期待，避免新兼容包装；F6 不改变合法 v3 wire JSON |
| D：目录与工程收敛 | F17/F19/F20/F21/F22 | canonical 时间、语法全集、安装失败恢复、同 SHA 发布证据；分别按行为验收 |

每个独立改动完成后及时提交/推送；提交前 cargo fmt --all，使用仓库 hook 和 scripts/test-fast.sh。跨模块批次、读写契约或广泛模型重构再运行 full 及受影响协议 gate。涉及平台真实文件/设备机制时安排对应平台原生/虚拟 HIL；实体盘需具体场景证据，不能用单元测试代替。不要先做大目录迁移、全 crate 拆分或删除全部兼容名称，再尝试修复行为。

## 本轮验证与可复现材料

- `python3 scripts/test-full.py --profile full --deadline-seconds 900 --max-seconds 900`：8 suites、10 artifacts、doctest，0 failures，35.28s；包含 runner 行为和 PTY replay/owned-process cleanup 检查。
- `cargo clippy --all-targets --locked -- -D warnings`：通过，4.09s；也编译示例等 targets，不能当成其他 OS 原生运行。
- `cargo deny check`：advisories/bans/licenses/sources 均通过；仅上述两个重复版本 warning。
- `python3 scripts/protocol/audit_baseline.py`：通过，20 unique images、19 strict encrypted images。
- `scripts/test-fast.sh`：格式/diff、Clippy、table-scroll 和受影响门禁通过；runner 为 3 suites、5 artifacts，0 failures，9.89s。
- F1/F3 有界内存复现：[源码](evidence/20261006-gpt-pin-probe.rs)、[结果](evidence/20261006-gpt-pin-probe.txt)。
- F9/F10 与权限、Unix 发布原语复现：[源码](evidence/20261006-container-contract-probe.rs)、[结果](evidence/20261006-container-contract-probe.txt)。原语复现不冒充私有 lineage persist 的并发端到端测试。

复现针对该审计基线，未来修复后断言应失效。仓库根执行 `cargo build --locked --lib` 后，可用 `rustc --edition=2021 <probe.rs> --extern edpcli=target/debug/libedpcli.rlib -L dependency=target/debug/deps -o <temporary-output>` 编译，再在仓库根运行。保留容器 probe 在仓库内的位置，以正确解析其引用的 restore_plan 源文件。Probe 仅用内存设备及系统临时目录，结束清理其临时容器；不包含物理盘路径、真实秘密或备份改写步骤。

未在本轮执行：Windows/Linux 原生运行、虚拟或实体盘 HIL、真实断电、安装、发布、迁移备份。以上未执行项作为具体实施的验证条件，不是阻止完成当前静态与非 HIL 审计的理由。
