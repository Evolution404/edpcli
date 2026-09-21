# edpcli — EDP/cems U 盘管理 CLI

Rust 单二进制工具，支持 **macOS / Linux / Windows**。三平台共享同一套 EDP/cems
识别、备份、元信息、扇区检查、转换与安全写入核心；操作系统差异统一收敛在
`src/platform/`。

CLI v2 的日常工作流只有五类任务：

```text
edpcli list       查看当前插入的 U 盘
edpcli info       查看 U 盘或备份详细信息
edpcli apply      预览或执行 U 盘改造
edpcli backup     创建、查看、校验、恢复和清理备份
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

完整安装、跨平台 selector、备份恢复和发布说明见
[`docs/USAGE.md`](docs/USAGE.md)。版本策略和 Release 门禁见
[`docs/RELEASE.md`](docs/RELEASE.md)。

## 安装 / 升级

macOS Apple Silicon 推荐直接从 GitHub 最新正式 Release 安装，并先校验 SHA-256：

```bash
tmp="$(mktemp -d)" && cd "$tmp"
gh release download --repo Evolution404/edpcli \
  --pattern 'edpcli-v*-macos-arm64.tar.gz' \
  --pattern 'edpcli-v*-macos-arm64.tar.gz.sha256'
shasum -a 256 -c edpcli-v*-macos-arm64.tar.gz.sha256
tar -xzf edpcli-v*-macos-arm64.tar.gz
sudo install -m 0755 edpcli /usr/local/bin/edpcli
edpcli version
```

其他平台/架构的安装命令见 [`docs/USAGE.md`](docs/USAGE.md)。

## 快速使用

```bash
edpcli
edpcli list
edpcli info
edpcli info --disk 4
edpcli info backup.bin

edpcli apply --dry-run
edpcli apply --dry-run --disk 4
edpcli apply --disk 4
edpcli apply --disk 4 --size 100
edpcli apply --disk 4 --force

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

edpcli inspect --lba 7
edpcli inspect --disk 4 --lba 6,7,12 --hex
edpcli inspect backup.bin --lba 7,12 --hex
edpcli inspect backup.bin --lba 7 --raw

edpcli convert --dir ./snapshot --id 'disk&ven_aigo&prod_u335' --out ./converted
edpcli version
```

## 交互式 TUI

```bash
edpcli tui
```

TUI 仅在交互式 TTY 中启动；其设备、备份和 Inspect 字段在渲染前统一过滤终端控制字符，
避免来自 U 盘元数据、文件名或系统探测文本的 ESC/换行等内容破坏 alternate-screen。
后台 worker 只通过 TaskHub 回传数据，不允许直接向 stdout/stderr 输出。核心键位：

- `j/k/h/l`：上下选择、切换设备/备份工作区或 Inspect 视图；
- `gg/G`：首项/末项；
- `Ctrl-d/Ctrl-u`：列表半页移动；Inspect hex 中滚动内容；
- `/` + Enter：搜索，`n/N` 前后匹配；
- `:`：任务型 command palette，不执行 shell；
- `i`：Inspect，支持字段、decoded hex、raw hex；
- `b`：为当前选中设备创建只读 LBA0-12 备份；
- `a`：Apply 安全向导；
- `R`：从当前备份执行 Restore 安全向导；
- `Esc`：返回，`q`：退出，`?`：帮助。

设备扫描、备份扫描和 Inspect 读取都在后台 worker 执行，不阻塞 redraw；同类设备/备份扫描使用 single-flight 去重，连续刷新不会无限创建线程。Backup create 复用现有只读 `backup_create_flow`。Apply / Restore
仍只调用 CLI 共用的 application write service。进入真实写盘前必须明确输入 `YES`；
需要提权时会固定平台原生 disk selector，Restore 还会固定精确备份路径，提权后的 TUI
再次要求 `YES`。进入关键写盘阶段后，`q` / `Esc` / `Ctrl-C` 只登记延迟退出，
不会中断卸载、reopen、atomic write、sync/readback 或 rollback。

## 备份

`backup create` 与 `apply` 写前自动备份共用同一个 `create_backup` service：

- 固定读取 LBA0-12，共 6656B；读取 2.2.0 生成的 7168B 旧备份时忽略尾部 LBA13；
- 使用相同的 onlyid、device_id、VID/PID、容量元数据；
- 使用相同命名和 `_nopwd` 状态标记；
- 写出相同 MD5 sidecar；
- 使用 create-new 防覆盖、fsync 和目录持久化；
- 独立备份路径只读 U 盘，不卸载、不锁卷、不 reopen 为读写、不写任何扇区。

`backup list` 使用统一的全局展示编号。相同编号语义用于 `verify`、`delete`
以及恢复时的备份选择；恢复仍会按当前物理盘身份过滤并以 LBA4 身份标签终验，避免同型号
U 盘串盘。

## 写盘安全

`apply` 与 `backup restore` 的真实写盘路径保持以下 fail-closed 门禁：

- 目标必须是外接 USB 整盘；
- 系统盘身份无法确认时拒绝继续；
- 提权前固定平台原生 selector；
- 写前读取 LBA0-12，并在 apply 时先创建自动备份；
- 写入前卸载/锁定目标卷；
- reopen 后再次核对介质和写前元数据；
- 原子写入、sync、逐扇读回校验；
- 任一写入失败自动回滚，回滚结果有独立退出码；
- 恢复备份必须通过大小、MD5 与当前盘 LBA4 身份终验。

`edpcli apply --dry-run` 复用真实识别和布局计算，但不会创建备份、请求写入确认、
卸载/锁卷、reopen 或写入扇区。

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

常用门禁：

```bash
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
```

CI 继续保留 `cargo fmt --all -- --check` 作为最终兜底，并在 macOS、Linux、Windows 的 arm64 / x86_64 六个目标上执行测试、clippy 和构建；
Linux/Windows 另有 arm64 / x86_64 virtual-disk HIL。正式 Release 同时发布六个原生包，
macOS 额外发布 Universal 包。
