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

无参数 `edpcli` 等价于 `edpcli list`。

完整安装、跨平台 selector、备份恢复和发布说明见
[`docs/USAGE.md`](docs/USAGE.md)。版本策略和 Release 门禁见
[`docs/RELEASE.md`](docs/RELEASE.md)。

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

## 备份

`backup create` 与 `apply` 写前自动备份共用同一个 `create_backup` service：

- 固定读取 LBA0-13，共 7168B；
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
- 写前读取 LBA0-13，并在 apply 时先创建自动备份；
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

补全会动态提供当前物理盘、备份全局编号、备份文件名和 LBA0-13，并与 CLI v2 parser
使用同一命令模型。

## 开发与验证

项目工具链由 `rust-toolchain.toml` 固定。常用门禁：

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

CI 在 macOS、Linux、Windows 的 arm64 / x86_64 六个目标上执行测试、clippy 和构建；
Linux/Windows 另有 arm64 / x86_64 virtual-disk HIL。正式 Release 同时发布六个原生包，
macOS 额外发布 Universal 包。
