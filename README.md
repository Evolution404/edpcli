# edpcli — EDP/cems U 盘管理 CLI

Rust 单二进制工具，支持 **macOS / Linux / Windows**。三平台共享同一套 EDP/cems
识别、备份、元信息、扇区检查、制盘与安全写入核心；操作系统差异统一收敛在
`src/platform/`。

CLI v2 的日常工作流包括：

```text
edpcli list       查看当前插入的 U 盘
edpcli info       查看 U 盘或备份详细信息
edpcli backup     创建、查看、校验、恢复和清理备份
edpcli provision  mode0～mode3 官方制盘、Plain 普通盘；mode1 可保留重制现有 mode0
edpcli inspect    高级：检查底层 LBA/hex 数据
```

在交互式终端中直接运行无参数 `edpcli` 会默认请求管理员权限并进入 TUI；也可以显式运行：

```bash
edpcli tui
```

管道、重定向或自动化等非 TTY 环境下，无参数 `edpcli` 仍保持 `edpcli list` 语义，
不会把现有脚本切进 TUI。TUI 基于 `ratatui + crossterm`，跨 macOS / Linux / Windows
使用同一套 application/service。macOS/Linux 在进入 alternate screen 前通过 `sudo`
请求权限，Windows 通过 UAC；授权后一次进入完整能力 TUI。

完整文档分类见 [`docs/README.md`](docs/README.md)。安装、跨平台 selector、备份恢复见
[`docs/user/USAGE.md`](docs/user/USAGE.md)，版本策略和 Release 门禁见
[`docs/user/RELEASE.md`](docs/user/RELEASE.md)。

## 安装 / 升级

macOS Apple Silicon 推荐直接从 GitHub 最新正式 Release 安装，并先校验 SHA-256：

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

本机开发/测试构建不要直接写 `/usr/local/bin`。在仓库根目录一条命令安装当前工作区：

```bash
make install
```

不想依赖 `make` 时可直接执行：

```bash
./scripts/install.sh
```

这两个入口使用同一条安装链：`cargo build --release --locked` 构建当前工作区，
随后复用 `scripts/install-local.sh` 安装到 `~/.local/bin/edpcli`，并校验交互式
zsh 的实际命令解析结果与 SHA-256。安装命令不会自动执行 `git pull`、切换分支或
修改工作区；存在未提交修改时会明确提示并安装当前工作区实际代码。

其他平台/架构的安装命令见 [`docs/user/USAGE.md`](docs/user/USAGE.md)。

## 快速使用

```bash
edpcli
edpcli list
edpcli info
edpcli info --disk 4
edpcli info backup.edpb

edpcli backup create
edpcli backup create --disk 4
edpcli backup list
edpcli backup restore
edpcli backup restore 2 --disk 4
edpcli backup verify
edpcli backup verify 2
edpcli backup delete
edpcli backup delete 2,4,5
edpcli backup prune --keep 2

edpcli provision plan --disk 4 --target mode1 --share-mib 1024 --encrypt-mib 2048 \
  --label-id 1402259934 --user USER06 --dept '江苏省电力有限公司' \
  --label '江苏电力!SAFE6' --password '你的密码'
edpcli provision write --disk 4 --target mode1 --share-mib 1024 --encrypt-mib 2048 \
  --user USER06 --dept '江苏省电力有限公司' --yes
edpcli provision plan --disk 4 --target mode1 \
  --force-change-password --cancel-password-complexity-check \
  --share-max-password-errors 5 --encrypt-max-password-errors 5
edpcli provision plan --disk 4 --target plain
edpcli provision write --disk 4 --target plain --partition 2048:512MiB:exfat:DATA --yes
edpcli provision image --disk 4 --target plain --out ./edp-plain.img
edpcli inspect meta --lba 7
edpcli inspect raw --disk 4 --lba 240250283-240250288
edpcli inspect decode --disk 4 --lba 20480 --count 8
edpcli inspect meta backup.edpb --lba 7,12

edpcli version
```

## 交互式 TUI

```bash
edpcli tui
```

TUI 仅在交互式 TTY 中启动；设备和备份两个标签页使用同一套
application/service，不维护第二套业务实现。来自 U 盘元数据、文件名、快照路径和系统探测
的文本在渲染前统一过滤终端控制字符，后台 worker 只通过 TaskHub 回传数据，不允许直接向
stdout/stderr 输出。

TUI 已覆盖设备列表/详情、全盘检查、元数据备份、校验、恢复、单条/批量删除、keep-N 清理、官方四模式制盘、Plain 普通盘目标、现有盘重制与数据迁移，以及目标绑定稀疏镜像导出。设备页 `Enter` 打开设备详情，`p` 进入制盘方案，`i` 进入只读全盘检查，`b` 创建元数据备份。物理写盘统一遵循“先只读计划/预览，再精确输入 YES”的交互。

顶层 `Tab/Shift-Tab` 在设备和备份标签间切换；全盘检查内循环结构树、节点概览和节点详情，`Ctrl-w h/j/k/l/w/W` 用于局部面板焦点。备份页 `Enter/i` 检查、`b` 新建备份、`v` 校验、`R` 恢复、`d` 删除。`q` 发出全局退出意图，`Esc` 返回面包屑标明的上一级；真实写盘关键阶段仍延迟退出到安全检查点。所有表格统一用 `j/k` 选行、`h/l` 激活列、`</>` 将当前整列向左/右交换、`0/$` 跳第一/最后一列、`H/L` 每次按 2 个 terminal cell 横向移动视口、`s` 切换当前列正/倒序、`S` 恢复默认顺序；横向/纵向溢出时分别在表格底边/右边显示滚动条；结构树中的 `h/l` 仍用于层级操作，输入中 `h/l` 仍是文本。制盘 Form 的 Normal 模式用 `i` 编辑字段、`Enter` 生成只读计划；Insert 模式用 Enter/Esc 完成字段编辑。全盘检查中 `gl` 跳转，扇区检查器用 `v` 循环 Raw/Decode/Mixed。

设备扫描、备份扫描、快速/高级检查、制盘计划和镜像导出均在后台 worker 执行，
不阻塞 redraw；同类任务使用 single-flight，繁忙期间只保留最新有效结果。动画只重绘可见表格行，
可通过 `EDPCLI_ANIMATION=reduced` 降低更新频率，或用 `EDPCLI_ANIMATION=off` 关闭动态帧。
进入关键介质事务后，`q` / `Esc` / `Ctrl-C` 只登记延迟退出，不会中断卸载、reopen、
atomic write、sync/readback 或 rollback；终端 I/O 失败时也会先恢复终端，再等待关键 worker
完成安全收尾。

## 备份

`backup create` 创建自包含、自校验的 `.edpb` 元数据备份：

- Plain 保存分区表原始元数据和类型化几何；
- EDP 保存 LBA0～12、LBA7 指向的 LCE 与已确认盘尾协议对象；
- 保存可验证的硬件/协议身份与精确容量；
- 不采集目录和用户文件内容；
- 完整性摘要保存在 EDPB 内部，不生成备份旁挂校验文件；
- 使用仅新建方式防止覆盖，并同步文件和目录；
- 独立备份只读 U 盘，不卸载、不锁卷、不以读写方式重开，也不写任何扇区。

`backup list` 使用统一的全局展示编号。相同编号语义用于 `verify`、`delete` 和恢复时的备份选择；恢复会按 EDPB 中的硬件/协议身份证据、逻辑扇区大小和总几何重新授权目标盘，不能仅凭同型号、容量或临时 `diskN` 放行。

损坏的 `.edpb` 同样保留全局编号，CLI/TUI 显示具体校验失败原因，不能作为恢复输入。读取会先固定通过校验的 manifest 和数据快照，再构建恢复计划；资源预算为容器 256 MiB、manifest 4 MiB、单项数据 64 MiB、累计数据 128 MiB、最多 1024 项，超过预算会明确拒绝。

删除会先将目标原子移入同目录的私有 `.edpcli-delete-<随机值>/entry.edpb`，然后核对确认时的摘要并删除。若校验失败，会尝试以硬链接恢复原名且不覆盖同名文件；文件系统不支持硬链接或原名已被占用时，错误中会给出保留路径。进程在隔离后异常退出，也可能留下该路径：检查并校验保留文件，使用新的文件名将它移回备份目录；确认恢复前请保留隔离目录。

## 写盘安全

`backup restore` 与 `provision` 的真实写盘路径保持以下无法确认即拒绝继续门禁：

- 目标必须是外接 USB 整盘；
- 系统盘身份无法确认时拒绝继续；
- 提权前固定平台原生 selector；
- 真实制盘在任何破坏性写入前强制创建并校验当前盘的 EDPB 元数据备份；
- 写入前卸载/锁定目标卷；
- reopen 后再次核对介质和写前元数据；
- 原子写入、sync、逐扇读回校验；
- 任一写入失败自动回滚，回滚结果有独立退出码；
- 新盘制盘原样保留目标制造商 LBA3，并在重开后再次核对；
- 保留重制时保持可复用分区的起点、大小和密钥材料不变；
- 恢复备份必须先完整验证 EDPB，再通过强物理身份、逻辑扇区大小、总几何及适用的协议身份复核。

`edpcli provision plan` 只读计算目标布局，不卸载/锁卷、reopen 或写入扇区。

## 平台实现

- **macOS**：IOKit/IORegistry 获取 USB、UAS/BOT 与 SCSI 信息，`diskutil` 处理整盘信息
  和写前卸载，管理员权限由 CLI 自己请求。
- **Linux**：sysfs 与 `/proc/self/mountinfo` 识别块设备和系统盘关系，写前使用原生
  卸载逻辑，管理员权限由 CLI 自己请求。
- **Windows**：SetupAPI / Storage IOCTL 获取 PhysicalDrive 与系统卷关系，写前执行
  volume lock/dismount，通过 UAC 重执行。

业务层有平台边界门禁，禁止重新直接依赖 OS 专用命令、设备路径或 PowerShell。

## Shell 补全

```bash
# zsh
eval "$(edpcli completion zsh)"

# bash
eval "$(edpcli completion bash)"

# fish
edpcli completion fish | source
```

补全会动态提供当前物理盘、备份全局编号、备份文件名和 LBA0-12，并与 CLI v2 parser
使用同一命令模型。

## 开发与验证

项目工具链由 `rust-toolchain.toml` 固定。首次克隆后安装仓库管理的 Git hook：

```bash
# macOS / Linux
./scripts/install-git-hooks.sh
```

```powershell
# Windows
./scripts/install-git-hooks.ps1
```

安装器会设置 `core.hooksPath=.githooks`。之后每次 `git commit` 前，pre-commit
会自动对已暂存的 Rust 文件执行 `rustfmt --edition 2021` 并重新暂存；如果同一个 Rust
文件同时存在已暂存和未暂存修改，则 fail-closed，避免自动格式化把额外改动带进提交。
通过 GitHub/API 等不会执行本地 hook 的提交路径，仓库 `AGENTS.md` 仍要求在每次提交前
显式执行 `cargo fmt --all`。

日常验证与大范围重构门禁：

```bash
./scripts/test-fast.sh
# 合并、发布前，以及大范围重构后
python3 scripts/test-full.py --profile full
```

每次提交前显式执行 `cargo fmt --all`，CI 保留 `cargo fmt --all -- --check` 兜底。日常 CI 在 macOS arm64、Linux x86_64、Windows x86_64 三个主平台执行完整非 HIL 测试、Clippy 和发布配置检查；macOS x86_64、Linux arm64、Windows arm64 三个次平台执行全目标编译与发布配置检查。虚拟磁盘 HIL 单独覆盖 Linux/Windows 两种架构及 macOS arm64。正式发布另有六架构完整门禁，生成六个原生包和 macOS Universal 包，详见 [发布规范](docs/user/RELEASE.md)。
