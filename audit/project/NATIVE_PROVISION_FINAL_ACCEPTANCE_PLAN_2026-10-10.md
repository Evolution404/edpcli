# edpcli 统一原生制盘最终验收与发布执行计划（2026-10-10）

> **状态：待继续执行（交接基线）**。本文记录已验证事实、未闭合任务及进入下一阶段的门槛，不能用计划替代完成证据。
>
> 仓库：/Users/zhangyuxi/Desktop/edpcli
>
> 当前开发分支：feat/native-4kn-wal-staging-20261010
>
> 最近已推送基线：95415e4a（feat(provision): unify CLI and TUI native source-aware reprovisioning）
>
> 最终目标：Plain / Mode0 / Mode1 / Mode2 / Mode3 任意受支持来源 → 任意受支持目标，设备可为实体 USB 或显式选择的 macOS 虚拟 Disk Image；统一规划、统一原生 I/O、统一 WAL 事务；逻辑扇区大小只是设备几何参数。

## 一、不可更改的设计约束

1. **单条业务链**：设备发现 → 原生来源快照与独立分类 → NativePreparedProvision / 不可变写集 → 确认 → TargetSession 排他写租约及身份/容量/原生逻辑块复核 → NativeRawBlockDevice → WAL 持久化 → 原生块写入、同步、重新打开回读 → 结果与重新挂载验收。CLI、TUI、物理 USB、虚拟 Disk Image 必须复用同一应用层服务。
2. **--include-virtual 仅决定可见对象**：默认只枚举真实外部 USB；指定后显示已验证的 Virtual + Disk Image + WholeDisk 虚拟盘，其写入与真实 USB 同路径；不制造 HIL 特殊写盘器，也不因为 USB/虚拟类型不同改变制盘行为。
3. **原生块与协议字段分离**：原生 LBA/读写字节数采用设备实际 logical sector bytes；协议内部已验证的 512B 字段视图单独映射；禁止写死 512B 或 4096B 的执行分叉。支持 512*n 原生 I/O 不代表 FAT/exFAT 对任意扇区字节数可格式化。
4. **破坏性转换，非无损转换**：用户已授权丢弃来源用户文件；不得宣称保留原文件、原密码、旧 FileKey 或未知源区。来源 OnlyID 与可靠 PassInfo 等只在可证实时继承；用户显式设置优先。
5. **保留事务安全能力**：必须有身份重查、禁止系统盘、整盘判断、当前会话独占、单次破坏性确认、WAL/失败停止与重读验证；但不得用虚拟盘或非 512B 几何作为无理由的功能拦截。
6. **不得破坏并行工作**：不新建 worktree，不执行 git reset/clean，不覆盖他人未提交代码，不盲合并 PR；遵循 AGENTS.md 的记录、格式化、测试、提交和推送规范。
7. **证据等级不可混同**：普通稀疏文件仿真 ≠ macOS OS 层虚拟块设备 ≠ TUI 实际按键 ≠ 目标密码独立解锁 ≠ 真实 USB 主控实盘验收。

## 二、现有完成证据与准确状态

| 工作 | 已证实的范围 | 证据入口 |
| --- | --- | --- |
| 正式 CLI plan/image/write | 已接入原生来源识别、规划、WAL 写入业务 | src/cli/commands/provision.rs；src/application/provision/native_flow.rs |
| TUI 规划与提交 | 已复用应用制盘准备/写入服务，非 512B 只读分叉移除；尚缺整套真实按键验收 | src/tui/provision/task.rs、review.rs；src/application/provision.rs |
| 4Kn 系统级模式矩阵 | 25/25 来源→目标对通过；每组实际制来源、卸载重挂、制目标、再次重挂；合计 50 次 OS 原生块写入 | scripts/ci/macos-native-mode-matrix-hil.sh 最近一次完整 Job 返回 0 |
| 512B/4096B 正式 CLI | 两种原生规格各连续 Plain→Mode0→Mode1→Mode2→Mode3→Plain，合计 12/12，包含重新挂载及 Plain ExFAT 文件持久化 | scripts/ci/macos-cli-native-virtual-hil.sh 最近一次完整 Job 返回 0 |
| 稀疏文件 8 几何 | 512/1024/1536/2048/2560/3072/4096/8192B，每种 25 对，共 200 对离线重建；8 规格 WAL 恢复 | docs/architecture/NATIVE_VIRTUAL_MATRIX_ACCEPTANCE_2026-10-10.md |
| EDP→Plain 清理 | 已修复只覆盖 MBR 导致旧 LBA7/LBA12 和 LCE 残留；保留制造商 LBA3，LBA0 最后提交 | native_flow.rs，4Kn Mode3→Plain 独立重新识别验证 |
| Fast / Full | 历史成功轮次各 8 suites、10 artifacts、功能失败 0；冗余审计确认问题 0 | scripts/test-fast.sh；scripts/test-full.py；audit-redundancy.py |
| **最新一次 Fast 结果** | 8 个套件全部 PASS，但 85.27 秒 > 60.00 秒时限；所以整体门禁最终 FAIL，不能记为完全通过 | 最近 Fast Job 的 timing budget 输出 |
| 密码独立解锁、真实 TUI 按键、实体 USB | **尚无完整通过证据** | P1、P2、P5 待补 |

当前工作区基于 95415e4a，仍有 4 个未提交文件，下一执行者必须先审计且保留：
- src/application/provision/native_flow.rs：来源 OnlyID/可靠 PassInfo 继承逻辑；
- src/cli_args/help.rs：正式命令帮助文案；
- src/command_spec.rs：制盘命令摘要；
- src/tui/provision/result_render.rs：WAL 与 EDPB 展示区分。

## 三、P0——基线收口和测试门禁恢复（必须先做）

**执行顺序**
1. 核对 git status --short --branch、HEAD、远端、git diff、正在运行的其他 AI 任务，识别上述四文件是谁改的、当前是否被并发更新。不得 reset/clean 或跨分支覆盖。
2. 逐项审核未提交代码：显式参数 > 可信来源字段 > 新盘默认值；不继承未校验的 OnlyID/PassInfo；显示 WAL 原始变更块事务记录而非冒充 EDPB 备份。
3. 检查 CLI 帮助是否仍含“Mode0→Mode1 必须保留原密钥”“仅可写物理盘”之类与破坏性统一路径冲突的描述；更新项目架构说明，但保留带日期的历史证据。
4. 查明 Fast 85.27s 触发 60s 门禁：区分并行编译、冷缓存、脚本时间预算与真实测试退化。不得仅提高预算掩盖回归；记录热缓存/冷缓存、套件耗时与实际返回码。
5. 执行 cargo fmt --all、cargo fmt --all -- --check、git diff --check、scripts/test-fast.sh、uv run --locked python scripts/test-full.py --profile full、uv run --locked python scripts/audit-redundancy.py --check；每个异常修复后复测。
6. 以小步提交推送至现有开发分支，形成独立可复现的干净基线。

**完成门槛**：四个文件处理完毕、工作区干净、Fast/Full/审计最终退出码为 0，Fast 时限 PASS，记录实际用时及 commit SHA；若环境阻塞必须明确标记为未完成。

## 四、P1——密码域、FileKey、LCE 和解密内容闭环（首要功能正确性）

### P1.1 模式/加密规则统一表
- 逐一核实 Plain、Mode0、Mode1、Mode2、Mode3 对应的引导/交换/保密/二合一区及是否物理加密，不允许明文二合一区错误加密。
- 明确 FileKey 包裹算法与分区底层数据加密算法是**两层不同职责**；核对来源/目标密码域、密码变更策略、默认密码的特殊有效密码规则。
- 破坏性重建无需解开将丢弃的旧用户密文；必须生成新目标密钥并独立验证。若用户明确配置目标密码不可悄悄回退到默认值。
- 核对 OnlyID、设备标识、PassInfo 的可继承来源、默认值与显式覆盖优先级；非法或冲突元数据必须失败而不是静默兜底。
- 正式路径仅使用系统强随机；测试用固定密钥必须有清晰隔离，不可写入生产介质。

### P1.2 独立解密验收
- 从写盘后的真实原生块独立解析 LBA7/LBA12 的密钥记录，使用每个目标域的正确密码执行 unwrap_file_key 和 CRC 验证。
- 对每个可加密分区重新打开读取真实数据，使用对应的底层数据算法独立解密文件系统结构；检查 FAT16/FAT32/exFAT 文件系统启动扇区与元数据，不仅比较写入程序生成的 raw bytes。
- 核对 LCE 地址、长度、独立解密后的 3072B 内容；4Kn 尾部 1024B 采用现有经验证的明文零后加密规则；512B 分六个扇区，不能错用 512B LBA。
- 覆盖 FileKey wrapper 的当前受支持枚举、正确密码、错误密码、损坏 CRC、错误加密算法、错误 LCE 地址以及被损坏文件系统。
- Raw/Decode/Mixed 对于**不需要解密**的分区必须一致；需要解密的分区按真实变换关系标色。

**完成门槛**：五个目标模式在 512B 和 4096B 上均有独立解密、错误密码负例、文件系统元数据解析及 LCE 解密的日志和测试断言。没有独立解锁证明时不得写“密码解锁验收通过”。

## 五、P2——TUI 真实交互及 CLI/TUI 一致性

1. 创建本次独有的虚拟磁盘，确认系统返回 WholeDisk=true、Internal=false、Virtual、BusProtocol=Disk Image、DeviceBlockSize/TotalSize 与镜像完全一致；记录当前盘号，写前再次校验。
2. 实际启动 edpcli tui --include-virtual，逐步操作 Devices→Provision：模式选择，起点与容量（MiB/GiB/sector），OnlyID、密码、算法、文件系统/格式化配置，表单焦点，确认，提交，进度，结果，Esc 返回设备列表。
3. 对比同参数的 CLI plan 与 TUI 实际写集：原生块大小、分区起点/长度、LCE、密码域、文件系统、加密选项、OnlyID、写集块数与原生块哈希。必须**结构化一致**，不能只比较 UI 文本。
4. 检查 Tab/hl/jk、Esc、q、输入框、表格横向到首末列、激活列标题及滚动条，不能在改造中退化到只读草案。
5. 正确展示 WAL 文件而不是 EDPB；错误时显示 MediaIntermediate/MediaStateUnknown 等真实状态，不出现虚假的“已创建完整元数据备份”。
6. 写完卸载、重挂，重新检查目标模式及文件系统；重复多模式切换，覆盖 Plain 与所有 Mode。
7. 用 TUI 自动化/PTY 真事件回放配合测试；如果无法直接执行 GUI/交互，留下准确的未通过部分，不得用只读单元测试冒充全程验收。

**完成门槛**：有真实交互事件日志、CLI/TUI 同规划证明、实际制盘重挂读回、回到设备列表及失败返回路径的证据。

## 六、P3——全模式 × 原生几何 × 文件系统验收

1. 512B 的 OS 真实 Disk Image 跑完整 Plain/Mode0/Mode1/Mode2/Mode3 五种来源×五种目标=25 对；4Kn 已通过 25 对，但 P1 修正后的新代码也要重新跑，二者均检查来源和目标的卸载重挂独立识别。
2. 512B/4096B：Plain 的 FAT16/FAT32/ExFAT 及 EDP 明文分区必须验证实际 macOS 卷挂载、创建真实文件、sync、unmount/reattach 和 SHA256 内容一致。加密分区采用独立解密测试，不把不能直接挂载密文当成写入失败。
3. 1024B/2048B：已知属于文件系统允许的标准扇区大小，但 macOS OS 设备是否支持须另测；能创建系统块设备才称 OS HIL，否则标成离线文件格式化/解析。
4. 1536B/2560B/3072B/8192B：保留原生 I/O、协议和 WAL 验证；按文件系统标准拒绝不合法格式化，不制造假的可挂载 FAT/exFAT。
5. 逐块核对 native LBA0–12、LCE 原生位置和尾部、跨块字段、MiB/GiB 转换、容量/扇区数，不允许 512B→4Kn 因单位出错产生八倍范围。

**完成门槛**：可机器复核的测试矩阵及每对真实来源/目标/几何/文件系统/回读结果。沿用 scripts/ci/macos-cli-native-virtual-hil.sh、scripts/ci/macos-native-mode-matrix-hil.sh，不再新建专用写盘器。

## 七、P4——WAL 中断恢复、身份锁定及失败停止

- 对**与正式 CLI/TUI 相同的提交代码**注入：WAL 创建前、快照部分落盘后、写入非 LBA0 中途、sync 前、LBA0 最后提交前后、读回不一致、介质几何变化、设备丢失、目录不可写/空间不足。
- 逐条证明原始块精确回滚或目标块完整提交；无法判断介质状态必须停止并明确报告，禁止无限重试。WAL 重复恢复、过期/损坏 WAL 也应被安全拒绝。
- 验证断开后插入不同镜像、设备编号复用、身份变化、容量/原生块大小变化时，重开阶段必须拒绝写入。
- 验证默认不显示虚拟盘、--include-virtual 可选且具备独立设备身份核验、物理盘与虚拟盘统一确认入口、系统盘不被误写。
- 测试脚本只允许写其**本次创建且每次已验证身份**的镜像；退出时卸载并清理本次目录；禁止对扫描到的不明 /dev/diskN 自动执行 dd。

**完成门槛**：512B/4096B 故障注入状态矩阵，明确 Committed/RolledBack/MediaIntermediate/MediaStateUnknown 对应返回码、WAL 和逐块校验，以及有限退出证明。

## 八、P5——分支、CI、版本、发布与物理盘边界

1. 首先查看 git status/HEAD/branch/worktree、GitHub 上相关 PR 和并行 AI 的最新提交；其他协议逆向分支（LCE/OEM 等）须按具体依赖审计，不一股脑合并；不建立冗余 worktree。
2. 升级日期化架构文档，将旧的“通用制盘仍是只读”“只允许 Mode0→Mode1”等历史阶段标注为已被替代，保留原测试证据及版本边界。
3. CI 保留 Mac 真 Disk Image 专项 HIL；Fast/Full 不能误触真实设备写入；带 sudo 的 HIL runner 权限必须清晰，CI 不可执行时应报阻塞。
4. P0–P4 实际通过后完成 cargo fmt、Fast、Full、冗余审计及 OS HIL，记录最终退出码、日志、耗时及 SHA256。
5. 小步合入 main、push，并核对 main 与 origin/main；版本按项目现有版本策略升级。发行安装必须使用 scripts/install-local.sh，确认 zsh -lic 'command -v edpcli' 指向 ~/.local/bin/edpcli，并核对 build 与 installed SHA256 一致。
6. 目前没有 4Kn 实体 U 盘：虚拟盘 25/25 不等于真实 USB 固件、VID/PID、拔插、硬件解锁和长期稳定性验收。物理盘缺口必须保留为“待实体设备”，不能写为全部通过。

**完成门槛**：main 提交/版本号、安装一致性、整套复测证据、已关闭 PR/分支清单、待实体盘验收项目及原因。

## 九、任务顺序、门禁和复测指令

| 阶段 | 阶段任务 | 进入下一阶段的验收条件 |
| --- | --- | --- |
| P0 | 四个脏文件收口、Fast 60 秒预算问题 | 无无关改动，分支已推送，Fast/Full/审计真正 PASS |
| P1 | FileKey/密码/LCE 正确性 | 正确密码解密、错误密码失败，全部目标分区数据独立核验 |
| P2 | TUI 真实按键验收 | 正式共享写入路径、完整交互、回读、结果页正确 |
| P3 | 全几何/全模式/文件系统 | 512B 和 4096B 各 25/25，文件系统正确、挂载读写证据 |
| P4 | 中断、WAL、身份变化和停止 | 负向故障无误写，日志及原块校验完整 |
| P5 | PR、发布和主分支收口 | 版本、测试、安装 hash 通过；物理 USB 未测如实记录 |

常规检查（在仓库根目录）：
- git status --short --branch；git log -5 --oneline；git diff --check
- cargo fmt --all；cargo fmt --all -- --check
- scripts/test-fast.sh
- uv run --locked python scripts/test-full.py --profile full
- uv run --locked python scripts/audit-redundancy.py --check

OS 真实虚拟盘 HIL（仅使用脚本新建并核实的虚拟盘，需受控 sudo）：
- bash scripts/ci/macos-cli-native-virtual-hil.sh
- bash scripts/ci/macos-native-mode-matrix-hil.sh

为每个 WebCodex 会话创建自己的 audit/ai-progress/YYYYMMDD-HHMMSS-<wc_sess_id>.log，逐个有意义步骤立即追加本地时间、事件与验证结果；日志本身按 AGENTS.md 不纳入 Git。每阶段完成后在本文增补实际提交 SHA、测试结果和未完成项；**不得预先把 P0–P5 全部标已完成。**

## 十、下一 AI 必须交付的最终清单

1. 当前分支/HEAD/推送/主分支最终状态和安装版本；
2. 五来源×五目标与扇区几何的实际执行表；
3. 密钥、密码、LCE、Raw/Decode 及目标真实数据解密证据；
4. CLI/TUI 同参数同规划及完整真实交互回放；
5. WAL 失败恢复和负向身份测试证据；
6. FAT/exFAT 文件挂载、真实写入、重挂后 Hash；
7. Fast/Full/审计/OS HIL 最终时长、退出码；
8. 缺少实体 USB 而仍待 HIL 的测试项目，不得冒充通过。

**每阶段均以实际终态日志和可复现测试为准，代码存在、测试写成或运行中不代表通过。**

## 十一、2026-10-10 P0 实际收口记录

- 基线：分支 `feat/native-4kn-wal-staging-20261010`，起始提交 `b797286f`；接手时保留并审阅四项既有未提交更改（继承 `OnlyID`、可靠 `PassInfo`、帮助文本及 `WAL` 结果展示）。
- 首次常规 `Fast`：全部 8 套件实际执行，文档治理有 3 项断言失败；耗时 63.81 秒，退出码 1。将日期化流程计划从 `docs/architecture/` 迁入 `audit/project/`，并同步修订架构说明，没有删除历史证据。
- 并行度实测：在原 60 秒限制不变的情况下，3 个工作进程使 `Fast` 达到 51.99 秒，8 套件和 10 个产物全通过，退出码 0；据此把工作进程默认值从 2 改为 3，而非放宽时间上限。
- 直接执行 `scripts/test-fast.sh`：54.40 秒，8 套件、10 个产物，失败 0，退出码 0；格式、差异、`Clippy` 和表格滚动检查均通过；证据为 `target/p0-fast-default-20261010.log`。
- 直接执行 `uv run --locked python scripts/test-full.py --profile full`：56.28 秒，8 套件、10 个产物及 `doctest` 均通过，退出码 0；证据为 `target/p0-full-20261010.log`。
- `uv run --locked python scripts/audit-redundancy.py --check`：检查 845 个文件，确认问题 0、待核查候选 212，退出码 0；证据为 `target/p0-redundancy-20261010.log`。
- 当前记录仅对应 P0：密码独立解锁、TUI 真实按键、最终 OS 模式矩阵、WAL 故障恢复与物理盘尚未由本次会话重新验收。

## 十二、2026-10-10 P1 原生块密码/FileKey/LCE 实验记录

- 只读独立验收模块 `tests/hil/native_cli_crypto_hil.rs`（纳入既有 `native_macos_4kn_virtual_hil`）（显式 `ci-virtual-disk`、`#[ignore]`，须校验所属 macOS Disk Image 的原生整盘设备）；并入既有 `scripts/ci/macos-cli-native-virtual-hil.sh`，由正式 CLI 唯一写入，脚本只创建和校验自身临时 `WholeDisk/Virtual/Disk Image`。
- 正式 macOS 原生块设备 512B/4096B 各经 `Plain → Mode0 → Mode1 → Mode2 → Mode3 → Plain` 六次连续提交及重挂，另外各插入一次不同交换/保密目标密码的 `Mode0 → Mode0`，总计 14 次实际制盘、14 次只读重新打开独立检查；最终退出码 0。证据 `target/p1-native-final-20261010.log`，并有历史中间复测 `target/p1-native-os-crypto-custom-metadata-20261010.log`。
- 每次重新从原始 LBA0–12 解析 `LBA7/LBA12` 和目标类型；检查加密分区的正确默认/自定义密码解包、`FileKeyCRC`、旧式 LBA7 密钥记录、错误密码、改变 CRC、错误封装算法及未知算法负例。独立用协议原语而非写入器的扇区变换对 FAT16/exFAT 启动扇区和 FAT 保留项进行解密/解析；并对损坏启动扇区及不应为明文的加密区做拒绝断言。
- 独立从 LBA7 指针确定 LCE 位置：512B 为 6 个原生扇区，4096B 为 1 个原生扇区。以原生物理字节地址解密：前 3072B 为既有标准明文，4Kn 额外 1024B 是明文零经加密后的密文字节；错误偏移地址解密不得等于预期。二合一区 `BootShareCombined` 的原始启动扇区始终等于解码明文，但其密码域 FileKey/LBA7 记录仍须正确验证。
- `Mode0 → Mode0` 两区显式密码不同（仅临时虚拟盘测试值 `P1Share2026!`/`P1Encrypt2026!`），已证实默认密码不能解包新 FileKey，不会静默回退。Plain exFAT 还执行 macOS 挂载、写入文件、同步、卸载、重挂与内容持久化。
- P1 涉及的当前可写底层加密算法是 SMS4；其它 FileKey 封装算法已有原语及回归金样，但尚不对未认证的 AES/AES_CROSS 写入作实体兼容承诺。仅证明已支持的原生写盘功能及独立解锁/解密，不等价于 USB 硬件认证。P2 的 TUI 真实按键及后续 P3/P4/P5 还未形成终态证据。

## 十三、P2 原生 4Kn 结果页修复及“保留不格式化”关键缺口（2026-10-10）

本轮在统一原生写盘路径实现官方 `bootSize=10` 的实际字节边界、动态 FAT12/FAT16 几何判定，并修复 TUI 原生结果页：保留来源的逻辑扇区宽度和已确认的 LCE 位置，分区表与全盘地图一致；原生 WAL 成功回读不再因缺少旧式逐分区报告而显示“未确认”；耗时按任务完成时刻结束。针对性结果测试通过。512B/4096B 正式 CLI macOS 虚拟块设备 14/14 次写入和独立回读通过，但不能把这一组 CLI 成绩当作最新 TUI 端到端完结证据。

用户发现当前实体 USB `disk7`（U335、512B、已注册 Mode1）在表单选择保留配置后，下一步却要求重新格式化。代码根因：`src/tui/provision/preflight.rs` 的预检查允许按几何、文件系统和密钥状态保留；`src/application/provision/native_flow.rs` 原生重建器不执行来源保留规划，而是原先在三个格式化标志均为否时强制全开；同时会给所有分区产生新的 FileKey/LBA7 密钥。原密文与新 FileKey 不匹配有数据不可读风险。

### 已完成安全收口

1. 由 TUI 原生请求显式标记“未勾选格式化意味着保留”，该意图不可被 CLI 的“缺省全部重建”策略覆盖；明确部分格式化也不能在新 FileKey 条件下混入旧密文。
2. 共享原生制盘服务在打开目标磁盘之前拒绝不具备来源透传能力的保留请求。原有表单只读几何草案、密码核验和配置编辑仍可执行；实际写入不再重新解释“保留”为“格式化”。
3. 表单显示“保留·待支持”，避免在能力缺失时承诺无损保留。针对全部四种官方模式的拒绝断言与 CLI 缺省重建正向断言已通过；当前真实 USB 未发生任何写入。

### 未完成：真实无损保留属于下一项 P2 阻断任务

- 将旧 `src/application/provision/prepare.rs` 已有的 `PreserveOpaque`、`PreserveVerified`、`RewrapVerified` 来源感知判断接入**同一条原生**准备／WAL／提交路径，禁止为旧数据重生成 FileKey。
- 对每个保留分区严格核验来源与目标的原生起始 LBA、块数、加密算法、文件系统及密码域；复用原 LBA7/LBA12 记录和 FileKey；仅需改密时，在已验证原密码前提下仅重新封装原 FileKey。
- 保留区不得生成 FAT/exFAT 初始化写集，不得擦除现存密文；新建／格式化分区仍须显式创建独立密钥、生成文件系统并以 WAL 回读。整盘覆盖与保留区域交叉应直接拒绝。
- 按 512B/4096B 同一业务入口分别验证 Mode1→Mode1 全保留、Mode0→Mode1 混合保留、改密不格式化，以及来源密码未知／错误、几何不一致、身份漂移、WAL 中途故障等负例，再重新启用写入门禁。
- 在该实现及独立磁盘回读证据齐备前，**绝不能将“保留·待支持”改为“已支持”，也不能为通过界面验收而重新启用自动格式化兜底**。
