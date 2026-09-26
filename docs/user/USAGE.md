# edpcli v2 使用教程

`edpcli` 是 EDP/cems U 盘管理命令行工具，支持 macOS、Linux、Windows。普通使用只需要
理解“设备、制盘、备份”三个对象，不需要手工输入内部 onlyid 或备份索引参数。

> `backup restore` 和 `provision write` 会真实写入物理盘。工具会执行系统盘保护、USB 整盘
> 校验、目标选择器固定、卸载/锁卷、重新打开后身份复核、事务写入、同步、读回校验和
> 失败回滚。任何关键事实无法确认时都会拒绝继续。

## 1. 安装

正式发布提供七套包：

| 平台 | 架构 | 产物 |
|---|---|---|
| macOS | arm64 | `edpcli-vX.Y.Z-macos-arm64.tar.gz` |
| macOS | x86_64 | `edpcli-vX.Y.Z-macos-x86_64.tar.gz` |
| macOS | 通用 | `edpcli-vX.Y.Z-macos-universal.tar.gz` |
| Linux | arm64 | `edpcli-vX.Y.Z-linux-arm64.tar.gz` |
| Linux | x86_64 | `edpcli-vX.Y.Z-linux-x86_64.tar.gz` |
| Windows | arm64 | `edpcli-vX.Y.Z-windows-arm64.zip` |
| Windows | x86_64 | `edpcli-vX.Y.Z-windows-x86_64.zip` |

每个包都有独立 SHA-256；发布清单还包含清单 / SBOM。macOS Apple Silicon 推荐安装
arm64 包，通用包仅用于同一二进制兼容两类 Mac 的场景。

### 从 GitHub 正式发布安装 / 升级（推荐）

安装了 GitHub CLI (`gh`) 后，macOS Apple Silicon 可直接下载 **最新正式发布**、校验
SHA-256 并安装：

```bash
tmp="$(mktemp -d)" && cd "$tmp"
gh release download --repo Evolution404/edpcli \
  --pattern 'edpcli-v*-macos-arm64.tar.gz' \
  --pattern 'edpcli-v*-macos-arm64.tar.gz.sha256'
shasum -a 256 -c edpcli-v*-macos-arm64.tar.gz.sha256
tar -xzf edpcli-v*-macos-arm64.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 0755 edpcli "$HOME/.local/bin/edpcli"
edpcli version
```

这是安装和升级的统一命令；再次执行会从 GitHub 最新正式发布下载并覆盖旧版。
Intel Mac 将 `macos-arm64` 改为 `macos-x86_64`；需要通用二进制时改为
`macos-universal`。Linux 同理选择 `linux-arm64` 或 `linux-x86_64`，并将校验命令改为
`sha256sum -c ...sha256`。

如果已经手工下载了发布包，macOS / Linux 可直接安装：

```bash
tar -xzf edpcli-vX.Y.Z-macos-arm64.tar.gz
chmod +x edpcli
mkdir -p "$HOME/.local/bin"
install -m 0755 edpcli "$HOME/.local/bin/edpcli"
edpcli --version
edpcli version
```

在本机开发/试用未发布版本时，统一使用：

```bash
scripts/install-local.sh target/release/edpcli
```

该脚本固定写入 `~/.local/bin/edpcli`，不会写 `/usr/local/bin`；安装后还会通过
`zsh -lic 'command -v edpcli'` 和 SHA-256 对比确认用户终端实际运行的就是刚安装的二进制。

Windows 将 `edpcli.exe` 放入固定目录并加入 `PATH`：

```powershell
edpcli.exe --version
edpcli.exe version
edpcli.exe list
```

`--version` 保持单行；`version` 输出版本、平台、架构、目标三元组、UTC 编译时间、
Git 提交、Rust 编译器和构建类型。

## 2. 最常用任务

### 2.1 查看当前 U 盘

```bash
edpcli
edpcli list
```

交互式终端中无参数 `edpcli` 默认请求管理员权限并进入 TUI；管道、重定向和自动化等
非 TTY 环境下，无参数仍保持 `list` 语义。显式 `edpcli list` 始终输出设备、容量、
接口、VID:PID、姓名、部门、EDP/cems 状态、免密状态、onlyid、EDPF 分区和已有备份数量。

`list` 不写盘。程序先无特权读取；只有实际遇到裸盘权限不足时才自动请求管理员权限，
无盘时不会无意义弹提权。

### 2.2 交互式 TUI

```bash
edpcli tui
```

TUI 是 CLI v2 的交互前端，不是第二套业务实现。交互式 TTY 中无参数 `edpcli`
默认进入 TUI；非 TTY 的旧脚本仍保持 `bare=list`。macOS/Linux 会在进入备用屏幕
前通过 `sudo` 请求权限，Windows 通过 UAC，因此不会再等到写盘操作时退出 TUI 后重启。
来自 U 盘、备份和系统探测的文本在渲染前统一过滤终端控制字符，后台工作线程也禁止直接
向标准输出/标准错误输出，避免异常元数据破坏 ratatui 屏幕。

TUI 顶部使用统一工作区导航，并以 类 Vim 的 tab/window/navigation 语义组织交互。常用键位：

| 键位 | 行为 |
|---|---|
| `j / k` | 当前列表、树或局部控件上下移动 |
| `h / l` | 普通表格按列横向滚动；检查结构树折叠/展开；输入中为文本字符；扇区检查器中移动字节光标 |
| `gg / G` | 当前列表或树的首项 / 末项 |
| `Ctrl-d / Ctrl-u` | 当前列表、树或详情半页移动 |
| `Tab / Shift-Tab` | 顶层在“设备 / 备份”标签间切换；全盘检查内循环切换“结构树 / 节点概览 / 节点详情” |
| `Ctrl-w h/j/k/l/w/W` | 当前功能页内部的面板方向或循环切换 |
| `/` | 搜索当前设备、备份或检查内容 |
| `n / N` | 下一个 / 上一个搜索匹配 |
| `:` | 打开任务型命令面板，不执行 shell |
| `r` | 刷新当前工作区 |
| `b` | 设备页或备份页新建备份，并选择元数据备份 / 深度备份 |
| `Enter` | 设备页进入制盘；备份页进入全盘检查；制盘 Form 的 Normal 模式生成只读计划，Insert 模式完成字段编辑；检查中的扇区节点打开扇区检查器 |
| `i` | 设备页/备份页进入全盘检查；制盘 Normal 模式下编辑当前可编辑字段 |
| `R` | 备份页进入恢复安全向导，不绕过既有确认与写盘安全链 |
| `空格` | 勾选 / 取消当前备份，用于固定路径+SHA-256 的多目标删除 |
| `d` | 删除当前或已勾选备份，统一进入安全确认 |
| `v` | 备份页校验；扇区检查器中循环 原始 / 解码 / 混合 |
| `gl` | 检查跳转；`g` 前缀只保留 `gg` 和 `gl` |
| `Esc` | 返回面包屑提示的上一层 / 结束当前字段编辑；不退出 App |
| `q` | 全局退出意图；真实写盘关键阶段延迟到安全检查点 |
| `?` | 帮助 |

TUI 全盘检查顶部显示整盘布局条和精确图例，根结构树按物理 LBA 排序。未知区域保持“未知区域”标签，不推断为空闲空间。选中 LBA0～12 等已缓存节点时，概览与字段详情立即更新；选中其它已知扇区时按需只读加载。所有显示的 LBA 范围采用闭区间 `[start..end]`，内部边界计算仍使用半开区间。面包屑显示当前选中路径，并单独标明 Esc 的真实返回目标。表格使用自适应列宽，`h/l` 按列边界横向滚动，首列和关键盘型列保留可见。

制盘流程当前统一包含五种目标状态：官方 mode0/mode1/mode2/mode3，以及“恢复普通盘（Plain）”；不再提供单独的“改造”或 Offline Convert 模式。
从设备页按 Enter 进入制盘流程时，会先显示当前目标盘此前是否已有 EDPB 保存记录，并让用户明确选择“先保存当前盘”或“不保存直接继续”。保存完成或用户明确跳过后才进入模式选择。
官方四模式填写分区/身份/密码参数；Plain 使用同一表单框架，可配置 1～4 个 MBR 主分区、独立起点/容量、gap、卷标和已支持的文件系统。两类目标都会先生成绑定当前 USB 身份与容量的只读 Review，再要求精确输入 `YES` 才进入真实写盘。TUI 默认密码为 `0000aaaa`，
卷标默认为“启动区”；密码和卷标都可直接修改。交换区/保密区等容量字段默认手动输入 MiB，
模式0启动区则使用精确扇区数。模式0首次进入表单时，保密区默认填入 `1024 MiB`，交换区按
当前盘容量和 LCE 自动填成可用的最大整 MiB 值；这只发生在初始化阶段。此后启动区、交换区、
保密区三个输入框完全独立，修改任何一个都不会联动改写其它输入框，右侧提示会实时显示可填
范围以及当前布局的剩余空间或超出空间。按 `空格` 可切换为“按比例分配”，此时输入当前模式
中参与比例分配的分区权重（例如模式0的交换区与保密区可填 `1:2`），程序会在保留精确启动区
并避开 LCE 后按权重自动分配剩余容量。TUI 的 PassInfo 区域提供四项设置：**初始化密码强制修改**、**取消密码复杂性验证**、**交换区密码最大错误次数**、**保密区密码最大错误次数**。普通盘默认分别为“否、否、255、255”；已注册盘只有在 LBA7/LBA12 两份 PassInfo 可可靠解析且四项完全一致时才自动继承。两个最大错误次数输入范围均为 `0..255`。计划页可以按 `e` 导出与当前目标
硬件身份绑定的稀疏镜像，真实写盘仍需再次输入 `YES`。模式1若识别到目标盘已经是模式0，会自动改用保留重制计划：原 type4 的起点、大小和密钥材料保持不变，type2 从 LBA63 扩满到原 type4 之前；不会再出现单独的改造入口。

命令面板支持 `:devices`、`:backups`、`:provision`、`:inspect`、
`:backup-create`、`:backup-deep`、`:backup-verify`、`:backup-delete`、`:batch-delete`、`:backup-prune`、
`:restore`、`:refresh`、`:help`、`:q`。输入永远不会传给系统命令解释器。

TUI 只有一个用户可见的检查入口：`:inspect`、设备/备份页 `i` 以及备份页 Enter 都进入同一个**全盘结构树**。树可展开“设备 → 区域 → 范围 → 扇区 → 字段”，`o`/`h`/`l` 折叠展开，`gl` 跳转任意 LBA/绝对字节偏移，`/` 搜索结构化节点或已缓存字段/值，`n/N` 在全部匹配间循环；扇区检查器支持 `0/$` 行首尾、`gg/G` 扇区首尾、`Ctrl-u/Ctrl-d` 半页、`PageUp/PageDown` 在当前 sector 内整页移动、`[`/`]` 切换前后 sector、`v` 切换原始/解码/混合，以及 `Space/o` 展开字段/位。

设备/备份扫描、快速/高级检查、制盘计划和稀疏镜像导出都在后台执行；
同类任务使用 single-flight，旧代次结果不会覆盖较新的状态。所有真实介质写入都复用 CLI 共用的
application write/provision service，并保持以下安全链：启动 TUI 前完成管理员提权；固定目标盘与
onlyid/device_id；先只读预览；精确输入 `YES` 后才进入关键事务；关键阶段重新检查系统盘/USB
整盘、写前保护、卸载/锁卷、reopen 后身份、atomic write、sync/readback/rollback。关键阶段内
`q`、`Esc`、`Ctrl-C` 只登记延迟退出，终端异常时也会先恢复终端再等待关键 worker 安全结束。
备份单删、空格多选后的批量删除与 keep-N 清理都基于固定 SHA-256 计划执行，不会删除确认后被替换的文件，并继续保留
“每块盘至少 1 份备份”的底线。

### 2.3 查看详细信息

```bash
edpcli info
edpcli info --disk 4
edpcli info backup.bin
```

`info` 的来源选择规则：

1. 给出备份文件时查看该文件；
2. 给出 `--disk` 时查看指定物理盘；
3. 只有一个可用 U 盘时自动选择；
4. 多盘时显示交互菜单；
5. 没有 U 盘时提示插入设备，不自动猜测某份备份。

输出分为 **设备 / 身份 / 状态 / 备份** 四块，包括 onlyid、device_id、Dept、User、
SAFE6、分区和当前盘匹配备份数量。

### 2.6 官方四模式、普通盘与 mode1 保留重制

`provision` 同时支持四种官方模式和 Plain 普通盘目标。四种官方模式与官方工具一致：

| 模式 | 含义 | 逻辑分区 |
|---:|---|---|
| `0` | 缺省三分区 | type1 + type2 + type4 |
| `1` | 启动区和交换区二合一 | type2 + type4 |
| `2` | 整盘加密 | 兼容 type1 + type4 |
| `3` | 内外网通用双分区 | type1 + type2 |

先只读检查目标和布局：

```bash
edpcli provision plan --disk 4 --target mode1 \
  --share-mib 1024 --encrypt-mib 2048 \
  --label-id 1402259934 --user USER06 \
  --dept '江苏省电力有限公司' --label '江苏电力!SAFE6'
```


CLI 的正式目标参数是 `--target mode0|mode1|mode2|mode3|plain`；原有
`--mode 0|1|2|3` 继续作为四种官方模式的兼容输入。**Plain 是独立目标，不是 mode4，
`--mode 4` 与 `--target mode4` 都会拒绝。**

恢复普通盘时，未指定分区默认建立 P1：从 LBA2048 占满到盘尾、exFAT、卷标“普通卷”。
也可以重复使用 `--partition START:SIZE:FS[:LABEL]` 建立 1～4 个 MBR 主分区；
`SIZE` 支持扇区数、`MiB`、`GiB` 和 `fill`，显式起点之间的空间会保留为空闲区：

```bash
edpcli provision plan --disk 4 --target plain
edpcli provision plan --disk 4 --target plain \
  --partition 2048:512MiB:exfat:DATA \
  --partition 1100000:fill:fat16:TOOLS
edpcli provision image --disk 4 --target plain --out ./edp-plain.img
edpcli provision write --disk 4 --target plain --yes
```

Plain 的 `plan/image/write` 与 TUI 使用同一套 application 制盘事务：整盘 USB/系统盘
保护、目标身份与容量复核、写前 reopen、逐扇区读回以及失败回滚都不在 CLI 内重复实现。
Plain 镜像同样保留目标盘原始 LBA3。


CLI 未指定 `--password` 时使用 `0000aaaa`，未指定 `--volume-label` 时使用“启动区”。
两项都可以显式覆盖。

模式 0 的启动区按扇区精确建模：默认从 LBA63 开始占用 **20417 扇区**，因此下一分区
从 LBA20480 开始。TUI 直接显示并允许编辑“启动区扇区”；CLI 可用
`--boot-sectors N` 显式指定，mode0 未指定 `--boot-mib/--boot-sectors` 时默认
`20417`。

标签标识（onlyid）同样允许手动修改。若当前目标盘扫描到了 onlyid，TUI 默认沿用该值；
只有扫描不到时才生成一个合法的随机候选。CLI 未传 `--label-id` 时也会生成候选值。

PassInfo 四项策略均可通过 CLI 显式覆盖；未指定时，已注册盘继承可靠来源值，普通盘使用默认值：

```text
--force-change-password / --no-force-change-password
--cancel-password-complexity-check / --enforce-password-complexity-check
--share-max-password-errors N
--encrypt-max-password-errors N
```

两个最大错误次数的 `N` 必须为 `0..255`。默认不会要求首次插入后再次改密码，也不会取消密码复杂性验证；两区最大错误次数默认都是 `255`。

导出与该目标盘绑定的稀疏制盘镜像：

```bash
edpcli provision image --disk 4 --target mode1 \
  --share-mib 1024 --encrypt-mib 2048 \
  --label-id 1402259934 --user USER06 \
  --dept '江苏省电力有限公司' --label '江苏电力!SAFE6' \
  --password '你的密码' --out ./edp-mode1.img
```

真实制盘把 `plan` 改为 `write`。该操作是破坏性的：程序会固定目标 USB 整盘、容量和
USB/SCSI 身份，保留目标盘原有 LBA3 制造商元数据，生成随机文件密钥、旧版兼容密钥和
协议随机材料；写入时先提交文件系统与 LCE，再提交 LBA1-LBA12，最后提交 MBR。
所有触碰扇区都会先镜像，写入或读回失败时整组回滚。

当前产品真实格式化写入开放已经验证的 FAT16/exFAT 路线；加密分区使用已验证的 mode2(SM4) 扇区变换。协议模型能够描述更多文件系统和封装类型，但真实写入不会把“可描述”当作“已验证”。

已有模式0官方盘需要重制为二合一盘时，直接选择 mode1：

```bash
edpcli provision write --disk 4 --target mode1 \
  --share-mib 1024 --encrypt-mib 2048 \
  --user USER06 --dept '江苏省电力有限公司'
```

`write --target mode1` 会先只读识别现有布局（`--mode 1` 仍兼容）。若确认源盘为 mode0，则普通 mode1 的新盘容量参数不参与最终几何：程序保持原 type4 起点、大小和密钥材料不变，不移动或重加密 type4；LBA63 到原 type4 起点前的区域重建为空的明文 exFAT，并按二合一 type2 写满前部。**当前版本不会迁移原 type1/type2 中已有的用户文件**。TUI 会在进入模式选择前询问是否创建 EDPB 保存；CLI 如需保存可先执行 `edpcli backup create --disk 4`。

### 2.7 管理备份

```bash
edpcli backup create
edpcli backup create --disk 4
edpcli backup list
edpcli backup restore
edpcli backup restore 2 --disk 4
edpcli backup restore backup.bin --disk 4
edpcli backup verify
edpcli backup verify 2
edpcli backup verify backup.bin
edpcli backup delete
edpcli backup delete 2
edpcli backup delete 2,4,5
edpcli backup delete backup.bin
edpcli backup prune
edpcli backup prune --keep 3
edpcli backup prune --keep 3 --yes
```

## 3. `backup create`：立即备份当前 U 盘

`backup create` 是纯只读介质流程。多盘时使用和其他物理盘命令相同的
`DeviceSelector`；读取裸盘需要权限时由 CLI 自己提权并固定平台原生目标选择器。

已注册 EDP 盘继续创建元数据级或深度级 EDPB；当 LBA7/LBA4 已被 Plain 制盘清理时，普通
`backup create` 改为创建核心级 EDPB，并从 USB/SCSI 硬件信息生成 `device_id`。Plain 盘必须
能读取稳定的硬件序列号；备份只保存其 SHA-256 绑定，不把明文序列号写进容器。Plain 不具备
EDP 分区语义，因此 `backup create --deep` 会明确拒绝，而不会伪造深度备份结果。

它与写前备份共用同一套 EDPB 写入和持久化约束：

- 输入固定为 LBA0-12，`13 * 512 = 6656B`；
- onlyid 从备份自身 LBA4 重新解析；Plain 允许 onlyid 为空；
- 相同 device_id / VID / PID / 容量元数据；
- EDP 盘在可取得硬件序列号时也写入不可逆的序列号哈希绑定，供后续 EDP→Plain 后恢复终验；
- 文件名按状态使用 `_nopwd` 或 `_plain` 标记；
- EDPB 容器内工件与文件级 SHA-256 完整性校验；
- 相同仅新建方式防覆盖；
- 相同 `fsync` 与目录持久化。

独立备份不会调用写盘准备/卸载/锁定，不会重新打开为读写，不会修改 U 盘。

## 4. 备份列表和全局编号

```bash
edpcli backup
edpcli backup list
```

`backup` 缺省等价于 `backup list`。列表按物理盘分组，但每份备份使用一个**全局稳定
展示编号**。该编号在同一次目录状态下同时供 `verify`、`delete` 和恢复选择使用，
不会在每个 onlyid 分组里重新从 1 编号。

每项显示时间、原始/免密状态、健康状态和真实文件名，分组同时展示型号、onlyid、Dept、
User。备份健康检查包含固定大小和 EDPB 容器完整性校验。

备份目录优先级：

1. `--backup-dir <目录>`
2. `EDPCLI_BACKUP_DIR`
3. `~/.edpcli.conf` 中的 `backup_dir = 路径`
4. `./backup`

## 5. 恢复

```bash
edpcli backup restore
edpcli backup restore 2
edpcli backup restore backup.bin
edpcli backup restore --disk 4
```

无备份参数时：

1. 自动选择或交互选择目标 U 盘；
2. 读取当前盘 LBA4 身份；
3. 只显示属于当前盘的备份；
4. 用户选择；
5. 校验 6656B 大小、SHA-256 与 LBA4 身份；
6. 用户确认；
7. 卸载/锁卷、reopen 复核后执行原子恢复。

数字目标先经过统一 `BackupSelector`；若全局编号指向其他物理盘，恢复会拒绝。
当前 LBA4 身份非零时，显式文件也不能绕过原有 16B LBA4 身份终验。

若当前盘已经被转换为 Plain、LBA4 身份为零，则只允许**显式指定备份**进入恢复，并要求该
EDPB 带有新版本写入的硬件序列号 SHA-256 绑定；程序同时复核序列号哈希、VID/PID、容量和
当前 USB/SCSI 硬件能够生成的 `device_id` 候选，并在 unmount/lock 后、reopen 写入前再次复核。
旧 EDPB 若没有这项硬件绑定会继续 fail-closed，绝不会仅凭容量或同型号 VID/PID 放行。

免密状态快照会明确提示，并保持既有防误恢复语义。

## 6. 校验、删除和策略清理

### 校验

```bash
edpcli backup verify
edpcli backup verify 2
edpcli backup verify backup.bin
```

无参数校验全部；数字按全局编号选择；文件名或备份目录内路径精确选择。大小异常、SHA-256
缺失或不匹配返回备份错误退出码。

### 删除

```bash
edpcli backup delete
edpcli backup delete 2
edpcli backup delete 2,4,5
edpcli backup delete 2-4
edpcli backup delete backup.edpb
```

无参数进入全局编号交互多选。默认显示待删除内容并要求 YES；`--yes` 可在已经明确指定
目标时用于脚本。

删除安全门禁：

- 目标必须位于当前备份根目录；
- 确认后删除前再次比较扫描时的内容摘要，防止同名文件被替换；
- 只处理自校验 `.edpb` 容器，不再依赖外部摘要旁挂文件；
- 任一可识别物理盘组至少保留 1 份备份。

### 策略清理

```bash
edpcli backup prune
edpcli backup prune --keep 2
edpcli backup prune --keep 2 --yes
```

`prune` 默认只预览。加密原盘备份不会被自动清理；免密快照按每盘新旧顺序保留。
如果某盘没有加密原盘，即使保留数设为 0，也至少保留最新 1 份，避免清空。

## 7. 高级扇区检查

`inspect` 是只读盘面分析入口，必须显式选择一种模式：

- `raw`：读取物理 512B 原始扇区，不做任何解密或变换；
- `decode`：根据 LBA 所属区域执行已经验证的协议或数据区解码；算法、密钥或校验无法确认时直接失败，不会退回 raw 冒充解码结果；
- `meta`：显示物理偏移、区域叠加关系、分区几何、加密状态、协议字段和解码依据，不以十六进制字节流为主。

```bash
edpcli inspect meta --disk 4 --lba 7
edpcli inspect raw --disk 4 --lba 240250283
edpcli inspect raw --disk 4 --lba 240250283-240250288
edpcli inspect decode --disk 4 --lba 240250283 --count 6
edpcli inspect decode --disk 4 --lba 20480
edpcli inspect meta backup.edpb --lba 7,12
edpcli inspect raw backup.edpb --lba 240250283
edpcli inspect decode backup.edpb --lba 240250283 --export ./inspect-out
```

规则：

- `--lba` 使用非负十进制 `u64`，支持逗号列表和闭区间，如 `7,12,240250283` 或 `240250283-240250288`；
- `--count N` 只能与单个起始 LBA 同用，从该 LBA 起连续读取 N 个扇区；
- 单次最多检查 65536 个扇区，防止误输入造成无界读取；
- 对物理盘，LBA 必须满足 `0 <= LBA < total_sectors`，越界在读盘前拒绝；
- LBA0-LBA12 使用现有协议解析器；LCE 使用 zero8 + 64 位物理字节偏移 tweak；
- 数据分区把“所属区域”“`NeedEncrypt` 配置”和“物理数据当前是否为密文”分开判断，`NeedEncrypt=1` 不再直接触发 SM4；
- 分区起始扇区若通过严格 FAT12/FAT16/FAT32/exFAT/NTFS boot-sector 结构校验，`decode` 直接返回物理明文，并明确标记“未执行 SM4”；
- raw 起始扇区不能确认明文时，只有 `EncryptMode=2`、FileKey 可按已验证规则解封且 `FileKeyCRC=PASS`，并且 SM4-ECB 解密后的起始扇区再次通过严格文件系统校验，才把该分区判定为 mode2 密文；
- 检查分区内非起始 LBA 时，会先读取同一分区起始扇区作为物理状态证据；离线 EDPB 若没有采集该起始扇区则 `decode` fail-closed；
- 当前自动 FileKey 解封只对已经验证的默认密码配置开放；非默认密码、未知加密模式或 raw/decoded 两边都不能确认时会明确拒绝 decode；
- 未知厂商区或没有经过验证的算法只允许 `raw`/`meta`，`decode` 会 fail-closed；
- 区域可以重叠，例如 LCE 同时可能位于盘尾取证窗口，`meta` 会同时列出；
- EDPB 离线检查可读取容器中已采集的原始扇区范围；没有采集到的 LBA 会明确报告不存在；
- `--export` 按模式分别导出 `LBA<n>_raw.*`、`LBA<n>_decoded.*` 或 `LBA<n>_meta.txt`；
- 离线文件无法自动确定 device_id 时可显式 `--id`。

未指定备份文件时只选择物理 U 盘，不会因为无盘而自动跳到备份目录猜来源。

## 8. `--disk` 目标选择器

`--disk` 同时接受统一编号和平台原生整盘目标选择器：

| 平台 | 示例 |
|---|---|
| macOS | `--disk 4`、`--disk /dev/disk4`、`--disk /dev/rdisk4` |
| Linux | `--disk 2`、`--disk /dev/sdb`、`--disk /dev/nvme1n1` |
| Windows | `--disk 3`、`--disk PhysicalDrive3`、`--disk '\\.\PhysicalDrive3'` |

跨提权边界前，程序会把选中的目标固定为当前平台原生目标选择器，避免重执行后枚举漂移。

## 9. 命令行补全

```bash
# zsh
eval "$(edpcli completion zsh)"

# bash
eval "$(edpcli completion bash)"

# fish
edpcli completion fish | source
```

补全动态提供：

- v2 一级命令和备份子命令；
- 当前物理盘目标选择器；
- 备份全局编号；
- 备份文件名；
- LBA0-12；
- 各命令允许的参数。

## 11. 三平台写盘边界

| 平台 | 系统盘确认 | 写前卸载/锁卷 | 提权 |
|---|---|---|---|
| macOS | 根卷到 APFS PhysicalStore | 整盘卸载 | CLI 请求管理员权限 |
| Linux | 根挂载设备链 | 卸载并复查 `mountinfo` | CLI 请求管理员权限 |
| Windows | 系统卷到磁盘扩展区 | 锁定 + 卸载卷 | UAC |

共同原则是拒绝继续：系统盘身份、目标卷归属、卸载/锁卷、介质身份或写后校验任一项
无法确认时，不进入下一写入阶段。

## 12. 常用退出码

| 退出码 | 含义 |
|---:|---|
| 0 | 成功或预览完成 |
| 1 | I/O / 运行时错误 |
| 2 | 参数或用法错误 |
| 3 | 目标不可用、非目标盘、系统盘或身份无法确认 |
| 4 | 已免密盘拒绝重复写入，需要 `--force` |
| 5 | 备份缺失、大小/SHA-256 异常或备份安全策略拒绝 |
| 6 | 写失败且回滚失败，需要人工恢复 |
| 7 | 写失败但完整回滚，可安全重试 |
| 130 | 用户取消 |

## 13. 发布与开发门禁

本地：

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
```

GitHub Actions 在 macOS/Linux/Windows 的 arm64、x86_64 六个目标执行完整测试、clippy
和发布构建；Linux/Windows 另有四套虚拟磁盘硬件在环。发布时生成七个包，并验证
SHA-256、SBOM、清单。版本和标签必须严格一致，详细流程见
[`RELEASE.md`](RELEASE.md)。
