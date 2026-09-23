# edpcli v2 使用教程

`edpcli` 是 EDP/cems U 盘管理命令行工具，支持 macOS、Linux、Windows。普通使用只需要
理解“设备、改造、备份”三个对象，不需要手工输入内部 onlyid 或备份索引参数。

> `apply` 和 `backup restore` 会真实写入物理盘。工具会执行系统盘保护、USB 整盘
> 校验、目标选择器固定、卸载/锁卷、重新打开后身份复核、原子写入、同步、读回校验和
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
sudo install -m 0755 edpcli /usr/local/bin/edpcli
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
sudo install -m 0755 edpcli /usr/local/bin/edpcli
edpcli --version
edpcli version
```

Windows 将 `edpcli.exe` 放入固定目录并加入 `PATH`：

```powershell
edpcli.exe --version
edpcli.exe version
edpcli.exe list
```

`--version` 保持单行；`version` 输出版本、平台、架构、目标三元组、UTC 编译时间、
Git 提交、Rust 编译器和构建类型。

## 2. 最常用的五个任务

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

常用键位：

| 键位 | 行为 |
|---|---|
| `j / k` | 上 / 下选择 |
| `h / l` | 切换设备/备份工作区；检查界面中切换字段/解码十六进制/原始十六进制 |
| `gg / G` | 首项 / 末项 |
| `Ctrl-d / Ctrl-u` | 列表半页移动；检查十六进制中滚动 |
| `/` | 搜索当前设备、备份或检查内容 |
| `n / N` | 下一个 / 上一个搜索匹配 |
| `:` | 打开命令面板 |
| `i` | 检查当前设备或备份 |
| `b` | 创建当前设备的只读 LBA0-12 备份 |
| `v` | 校验当前选中备份的大小与 SHA-256 |
| `D` | 删除当前选中备份（必须输入 `YES`） |
| `a` | 改造安全向导 |
| `R` | 恢复当前选中备份 |
| `Esc` | 返回上一层 / 取消输入 |
| `q` | 退出 |
| `?` | 帮助 |

命令面板只接受任务语义，例如 `:devices`、`:backups`、`:inspect`、
`:backup-create`、`:backup-verify`、`:backup-delete`、`:apply`、`:restore`、
`:refresh`、`:help`、`:q`；它不会把输入传给命令解释器。

设备和备份扫描、检查 LBA0-LBA12 读取全部在后台执行；同类任务使用单任务并发合并，繁忙期间
的新请求会合并为最后一次，旧代次的结果不会覆盖更新状态。列表每帧只构造可见行；
设置 `EDPCLI_ANIMATION=reduced` 可降低动画更新频率，设置 `EDPCLI_ANIMATION=off` 可关闭
动态帧。备份创建复用现有只读备份服务；改造 / 恢复则进入明确的安全向导：

1. 启动 TUI 前已完成平台管理员提权；
2. 固定当前目标磁盘、onlyid、device_id；恢复同时固定精确备份路径；
3. 输入 `YES` 后才允许进入关键操作；
4. 关键写盘阶段复用与 CLI 完全相同的系统盘保护、USB 整盘确认、写前备份、卸载/锁卷、
   重新打开后身份复核、atomic 写入、sync/读回和 rollback；
5. 关键阶段内 `q`、`Esc`、`Ctrl-C` 不会杀掉写盘工作线程，而是在安全结束点后再退出；
   终端 I/O 失败时先恢复终端，再等待关键 worker 完成安全收尾；
6. 备份删除会固定选中时的内容 SHA-256，删除前重新扫描并复核内容；如果同名文件被替换会拒绝，
   同时保留“每块盘至少 1 份备份”的安全底线，并同步删除对应 `.sha256` 旁挂文件。

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

### 2.4 先预览改造

```bash
edpcli apply --dry-run
edpcli apply --dry-run --disk 4
edpcli apply --dry-run --disk 4 --size 100
```

预览模式会执行完整目标识别、device_id 判定和分区布局计算，但保证：

- 不创建备份；
- 不进入 YES 写入确认；
- 不卸载或锁卷；
- 不重新打开为读写；
- 不写任何扇区。

### 2.5 执行改造

```bash
edpcli apply
edpcli apply --disk 4
edpcli apply --disk 4 --size 100
edpcli apply --disk 4 --force
edpcli apply --disk 4 --force --yes
```

真实改造顺序：

1. 确认外接 USB 整盘且不是系统盘；
2. 读取 LBA0-12；
3. 创建写前自动备份；
4. 用户确认；
5. 卸载/锁卷；
6. 重新打开为读写并再次核对介质身份和写前快照；
7. 原子写入目标扇区；
8. 同步并逐扇读回；
9. 任一步失败时按既定回滚协议恢复。

`--force` 只用于明确允许重复改造已免密盘；`--yes` 用于脚本化确认。

### 2.6 管理备份

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

它与改造写前自动备份共用唯一 `create_backup` service：

- 输入固定为 LBA0-12，`13 * 512 = 6656B`；
- onlyid 从备份自身 LBA4 重新解析；
- 相同 device_id / VID / PID / 容量元数据；
- 相同文件名和 `_nopwd` 状态标记；
- 相同 SHA-256 旁挂文件；
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
User。备份健康检查包含固定大小和 SHA-256 旁挂文件。

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
显式文件也不会绕过身份门禁，最终仍以当前盘和备份的 LBA4 16B 身份标签终验。

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
edpcli backup delete backup.bin
```

无参数进入全局编号交互多选。默认显示待删除内容并要求 YES；`--yes` 可在已经明确指定
目标时用于脚本。

删除安全门禁：

- 目标必须位于当前备份根目录；
- 确认后删除前再次比较扫描时的内容摘要，防止同名文件被替换；
- `.bin` 和对应 `.sha256` 配对处理；
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
- 分区起始扇区若通过严格 FAT16/FAT32/exFAT/NTFS boot-sector 结构校验，`decode` 直接返回物理明文，并明确标记“未执行 SM4”；
- raw 起始扇区不能确认明文时，只有 `EncryptMode=2`、FileKey 可按已验证规则解封且 `FileKeyCRC=PASS`，并且 SM4-ECB 解密后的起始扇区再次通过严格文件系统校验，才把该分区判定为 mode2 密文；
- 检查分区内非起始 LBA 时，会先读取同一分区起始扇区作为物理状态证据；离线 EDPB 若没有采集该起始扇区则 `decode` fail-closed；
- 当前自动 FileKey 解封只对已经验证的默认密码配置开放；非默认密码、未知加密模式或 raw/decoded 两边都不能确认时会明确拒绝 decode；
- 未知厂商区或没有经过验证的算法只允许 `raw`/`meta`，`decode` 会 fail-closed；
- 区域可以重叠，例如 LCE 同时可能位于盘尾取证窗口，`meta` 会同时列出；
- EDPB 离线检查可读取容器中已采集的原始扇区范围；没有采集到的 LBA 会明确报告不存在；
- `--export` 按模式分别导出 `LBA<n>_raw.*`、`LBA<n>_decoded.*` 或 `LBA<n>_meta.txt`；
- 离线文件无法自动确定 device_id 时可显式 `--id`。

未指定备份文件时只选择物理 U 盘，不会因为无盘而自动跳到备份目录猜来源。

## 8. 离线转换

```bash
edpcli convert --dir ./snapshot --id 'disk&ven_aigo&prod_u335' --out ./converted
```

`convert` 只处理快照目录，不访问物理盘，主要用于协议验证、金标比较和工程回归。

## 9. `--disk` 目标选择器

`--disk` 同时接受统一编号和平台原生整盘目标选择器：

| 平台 | 示例 |
|---|---|
| macOS | `--disk 4`、`--disk /dev/disk4`、`--disk /dev/rdisk4` |
| Linux | `--disk 2`、`--disk /dev/sdb`、`--disk /dev/nvme1n1` |
| Windows | `--disk 3`、`--disk PhysicalDrive3`、`--disk '\\.\PhysicalDrive3'` |

跨提权边界前，程序会把选中的目标固定为当前平台原生目标选择器，避免重执行后枚举漂移。

## 10. 命令行补全

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
