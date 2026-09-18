# edpcli 使用教程

`edpcli` 是 EDP/cems U 盘管理命令行工具，支持 macOS、Linux、Windows。三平台共享同一套
识别、备份、元信息、扇区检查、转换、写入与还原逻辑；平台差异只存在于设备枚举、权限、
系统盘识别和卸载/锁卷层。

> `apply` 和 `restore` 会真实写入目标物理盘。工具默认执行系统盘保护、USB 整盘校验、
> 写前备份、卸载/锁卷、重新识别、原子写入、同步、读回校验和失败回滚；任何关键身份或
> 安全状态无法确认时都会拒绝写盘。首次使用建议先执行 `list`、`meta`、`inspect` 和 `run`。

## 1. 下载与安装

每个 `v*` tag 都会由 GitHub Actions 自动构建并发布三套产物：

| 平台 | Release 产物 | 架构 |
|---|---|---|
| macOS | `edpcli-vX.Y.Z-macos-universal.tar.gz` | Apple Silicon + Intel Universal |
| Linux | `edpcli-vX.Y.Z-linux-x86_64.tar.gz` | x86_64 |
| Windows | `edpcli-vX.Y.Z-windows-x86_64.zip` | x86_64 |

每个压缩包旁都有同名 `.sha256` 文件，可在安装前核对下载完整性。

### macOS

```bash
tar -xzf edpcli-vX.Y.Z-macos-universal.tar.gz
chmod +x edpcli
sudo install -m 0755 edpcli /usr/local/bin/edpcli
edpcli --version
```

当前 Release CLI 未做 Apple Developer ID 公证。如果 Gatekeeper 阻止直接执行，优先选择
从源码构建；不需要也不应关闭 SIP。

### Linux

```bash
tar -xzf edpcli-vX.Y.Z-linux-x86_64.tar.gz
chmod +x edpcli
sudo install -m 0755 edpcli /usr/local/bin/edpcli
edpcli --version
```

### Windows

解压 `edpcli-vX.Y.Z-windows-x86_64.zip`，将 `edpcli.exe` 放到固定目录，并把该目录加入
`PATH`。随后在 PowerShell 或 Windows Terminal 中验证：

```powershell
edpcli.exe --version
edpcli.exe list
```

需要裸盘权限的命令会由程序请求 UAC 提权，不要求手工先用管理员终端启动。

### 从源码构建

需要 Rust 1.98 或更新版本：

```bash
git clone git@github.com:Evolution404/edpcli.git
cd edpcli
cargo test --all-targets
cargo build --release
```

二进制位于 `target/release/edpcli`；Windows 为 `target/release/edpcli.exe`。

## 2. 先认识 `--disk`

`--disk` 同时接受统一数字编号和平台原生整盘 selector：

| 平台 | 示例 |
|---|---|
| macOS | `--disk 4`、`--disk /dev/disk4`、`--disk /dev/rdisk4` |
| Linux | `--disk 2`、`--disk /dev/sdb`、`--disk /dev/nvme1n1` |
| Windows | `--disk 3`、`--disk PhysicalDrive3`、`--disk '\\.\PhysicalDrive3'` |

数字编号是 CLI 展示层的选择编号。跨提权重执行前，程序会把选中的目标固定成当前平台原生
selector，防止重枚举后数字编号漂移。

## 3. 第一次使用建议流程

### 第一步：列出外接盘

```bash
edpcli list
```

`list` 只读、不提权、不写盘。它会显示外接盘、容量、USB VID:PID、cems 识别、免密状态、
onlyid、EDPF 分区摘要和已有备份数量。

### 第二步：查看元信息

```bash
edpcli meta
edpcli meta --disk 4
```

常看字段包括：`onlyid`、`device_id`、VID:PID、容量、Dept、User、SAFE6、LBA7/LBA12
分区信息。也可以直接查看备份：

```bash
edpcli meta 1987718388
edpcli meta 1987718388 2
edpcli meta backup.bin
```

### 第三步：检查扇区结构

```bash
edpcli inspect
edpcli inspect 6 7 8 12 --disk 4
edpcli inspect 7 12 --disk 4 --hex
edpcli inspect backup.bin 7 12 --hex
```

`--hex` 查看解码后的字段感知 hex；`--raw` 查看原始密文/字节。两者不能同时使用。

### 第四步：先 dry-run

```bash
edpcli run
edpcli run --disk 4
edpcli run --disk 4 --size 100
```

`run` 执行与真实改造相同的识别和布局计算，但不提交扇区写入。建议第一次对某型号操作时
先确认它的目标盘、容量、device_id、onlyid 和预期分区布局。

### 第五步：真实 apply

```bash
edpcli apply
edpcli apply --disk 4
edpcli apply --disk 4 --size 100
```

真实写入顺序为：

1. 确认目标是外接 USB 整盘且不是系统盘；
2. 读取 LBA0-13 并创建写前备份；
3. 用户确认；
4. 卸载/锁定目标卷；
5. 再次核对设备和 LBA0-13 是否仍与写前快照一致；
6. 原子写入目标扇区并同步介质缓存；
7. 逐扇读回校验；
8. 任一步失败时尝试完整回滚。

脚本环境可加 `--yes` 跳过 YES 交互。已经是免密状态的盘默认拒绝重复 apply，需要明确
使用 `--force`：

```bash
edpcli apply --disk 4 --force
edpcli apply --disk 4 --force --yes
```

## 4. 备份管理

```bash
edpcli backup
edpcli backup list
edpcli backup list --onlyid 1987718388
edpcli backup verify
edpcli backup verify --onlyid 1987718388
edpcli backup verify --onlyid 1987718388 --index 2
edpcli backup prune
edpcli backup rm --onlyid 1987718388
```

备份固定包含 LBA0-13（14 × 512 = 7168B）并配套 MD5 sidecar。`verify` 会同时检查大小和
MD5。`prune` 默认只预览，只有加 `--yes` 才实际删除；加密原盘备份不会被自动清理，且
任何可识别盘组都不允许被清到 0 份备份。

备份目录优先级：

1. `--backup-dir <目录>`
2. `EDPCLI_BACKUP_DIR`
3. `~/.edpcli.conf` 中的 `backup_dir = 路径`
4. 当前目录下 `./backup`

## 5. 从备份还原

```bash
edpcli restore
edpcli restore --disk 4
edpcli restore backup.bin --disk 4
edpcli restore backup.bin --disk 4 --yes
```

`restore` 会校验备份大小、MD5、当前物理盘身份和 LBA4 onlyid。另一块同型号盘即使容量、
VID/PID、device_id 都相同，只要 onlyid 不匹配，也不会接受该备份。

## 6. 离线转换

```bash
edpcli convert --dir ./snapshot --id 'disk&ven_aigo&prod_u335' --out ./converted
```

`convert` 只处理快照目录，不访问物理盘，适合做协议验证、金标比较和回归测试。

## 7. Shell 补全

```bash
# zsh
eval "$(edpcli completion zsh)"

# bash
eval "$(edpcli completion bash)"

# fish
edpcli completion fish | source
```

补全包含子命令、onlyid、备份编号、LBA 和当前检测到的物理盘。

## 8. 三个平台的写盘安全机制

| 平台 | 系统盘确认 | 写前卸载/锁卷 | 提权 |
|---|---|---|---|
| macOS | 根卷 `/` → APFS PhysicalStore | `diskutil unmountDisk force` | `sudo` |
| Linux | 根挂载 major:minor + holder 链 | `umount2` + 再检查 mountinfo | `sudo` |
| Windows | Windows 系统卷 → volume disk extents | `FSCTL_LOCK_VOLUME` + `FSCTL_DISMOUNT_VOLUME` | UAC |

共同原则是 **fail-closed**：无法确认系统盘身份、目标卷归属、卸载/锁卷结果、介质身份或
写后校验时，不会继续写盘。

## 9. 常用退出码

| 退出码 | 含义 |
|---:|---|
| 0 | 成功或 dry-run/预览完成 |
| 1 | IO/运行时错误 |
| 2 | 参数或用法错误 |
| 3 | 目标不可用、非目标盘、系统盘或身份无法确认 |
| 4 | 已是免密状态，需 `--force` 才允许重复写入 |
| 5 | 备份缺失、大小/MD5 异常或备份安全策略拒绝 |
| 6 | 写失败且回滚失败，需要人工恢复 |
| 7 | 写失败但完整回滚成功，可安全重试 |
| 130 | 用户取消 |

## 10. 发布机制

仓库有两套 GitHub Actions：

- `Rust CI`：每次 push / PR 在 macOS、Linux、Windows 上执行全量测试、clippy 与 release build。
- `Release`：推送 `v*` tag 后重新执行发布级测试，并自动构建 macOS Universal、Linux x86_64、
  Windows x86_64 三套压缩包及 SHA-256，全部成功后创建同 tag 的 GitHub Release。

因此正式发布只需要在已经全绿的 `main` 上创建并推送版本 tag；任一平台构建失败都不会
创建不完整的 Release。
