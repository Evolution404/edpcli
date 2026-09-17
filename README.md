# nopwd_tool — cems 加密 U 盘 → 无密码盘

Rust 标准库实现，**零第三方 Rust crate**，单二进制 `nopwd`（macOS；复用系统自带
`diskutil` / `ioreg` / `sudo` / `iconv` 等工具）。

## 快速使用

```bash
cargo build --release            # 或 cargo install --path . 装入 ~/.cargo/bin
./target/release/nopwd list      # 列出外接盘：编号/容量/接口/cems 识别/免密检测/EDPF 分区/备份份数（免 sudo，sudo 下更全）
./target/release/nopwd run       # 预览改造（dry-run，自动检测 USB 盘）
nopwd run --disk 4               # 指定盘（接受 4 / /dev/disk4 / /dev/rdisk4）
nopwd apply                      # 实际写入（自动备份 → 原子写入 → 读回校验）
nopwd apply --disk 4 --size 100  # 指定盘 + Share 100GB
nopwd apply --force              # 盘已是免密盘仍强制重写（默认拒绝）
nopwd restore                    # 交互还原：列出本盘备份（新→旧，免密快照标注）→ 选择 → YES → 写入
nopwd restore <备份.bin> --yes   # 脚本化还原（自动确认）
nopwd backup                     # 默认就是 list：跨盘分组总览
nopwd backup list                # 显式写法；每份显示编号 + 真实文件名
nopwd backup list --onlyid ID    # 只查看某一物理盘
nopwd backup verify              # 全量校验备份（7168 字节 + MD5）
nopwd backup verify --onlyid ID  # 校验某一物理盘全部备份
nopwd backup verify --onlyid ID --index 2  # 只校验该盘第 2 份
nopwd backup prune               # 按策略预览旧免密快照（默认不删除）
nopwd backup rm --onlyid ID      # 显示该盘编号列表 → 选择 → YES 删除
nopwd backup rm --onlyid ID 2-3  # 按编号/范围删除；加 --yes 可脚本化
nopwd backup rm <备份.bin>       # 仍支持按文件名/路径精确删除
nopwd inspect                    # 有 U 盘则只读查看；没插盘则列出可离线查看的备份盘
nopwd inspect 6 7 12 --disk 14  # 展开物理盘指定扇区的结构化字段
nopwd inspect 7 12 --disk 14 --hex  # 追加字段感知高亮 hex
nopwd inspect --onlyid ID        # 先列出该盘有哪些 [1][2]... 可选备份
nopwd inspect --onlyid ID --index 2 # 按 backup list 编号查看某份备份
nopwd inspect <备份.bin> 11 12 --hex          # 文件可直接作位置参数，不提权
nopwd inspect 11 12 --backup <备份.bin> --hex # 仍支持显式 --backup
nopwd completion zsh             # 生成 zsh Tab 补全（bash/fish 同理）
nopwd convert --dir <快照目录> --id <device_id> [--out <目录>]   # 离线验证（不碰真盘）
```

### Tab 补全

补全不是静态命令表：`onlyid`、`--index`、备份文件名、当前物理盘号和 LBA0-13
都会由 `nopwd` 实时提供候选。

```bash
# zsh（当前 shell）
eval "$(nopwd completion zsh)"

# bash（当前 shell）
eval "$(nopwd completion bash)"

# fish（当前 shell）
nopwd completion fish | source
```

长期启用时，把对应命令放入 `~/.zshrc` / `~/.bashrc` / fish 配置即可。
子命令内部也支持聚焦帮助，例如 `nopwd inspect --help`、`nopwd backup --help`；
参数写错时只打印当前子命令的短帮助，不再刷整页全局教程。

`list` 效果（sudo 下 cems 盘认示三信号免密检测 `[免密]` 标记，并解密 LBA12
展示 EDPF 分区表——类型/LBA 范围/定义大小；原盘 3 条 Boot·Share·Encrypt，
转换后 2 条）：

```
$ nopwd list
外接盘 1 个:
  disk14  125.83GB  USB           3535:6300      cems盘 [免密]
                    └─ EDPF: Share 124.48GB (LBA 63~243,116,059) · Encrypt 1.34GB (LBA 243,116,060~245,734,654)
                       onlyid=1987718388 · 备份 3 份
```

`restore` 交互选单（带序号，免密快照/加密原盘双侧标记）：

```
$ nopwd restore
disk14 匹配备份 3 个(新→旧):
  1)  2026-09-17 22:41   [加密原盘]
  2)  2026-09-16 23:36   [免密状态]
  3)  2026-08-27 22:25   [加密原盘]
选择 [1-3] (回车取消):
```

终端输出带语义色（错误红/成功绿/警告黄/标记绿/降级灰/help 着色），
管道重定向或设置 `NO_COLOR` 时自动降级为纯文本。

- **自动提权**：`run` / `apply` / `restore` 需要裸盘读写，`inspect` 查看物理盘时需要裸盘只读；
  非 root 时自动以 `sudo`
  重执行自身（选定盘号并入参数，交互提示正常工作）；`list` / `backup` / `convert`
  以及 `inspect --backup/--onlyid` 永不提权。
- `--yes` 免交互；多块 USB 盘时自动弹编号选择；系统盘（disk<2）一律拒绝。
- `restore` 无论交互选择还是显式传入备份路径，写入前都以 LBA4 唯一身份标签终验当前盘；
  另一块物理盘的备份即使大小和 MD5 都正确也会被拒绝，防止同型号/误选文件串盘还原。
  同时要求对应 `.md5` 存在且校验通过；缺 sidecar 或摘要不符都不会进入写盘阶段。
- 备份目录（四级优先）：`--backup-dir` 旗标 > 环境变量 `NOPWD_BACKUP_DIR` >
  `~/.nopwd.conf` 的 `backup_dir = 路径` > `./backup`。
  - 自动提权时 sudo 会清环境变量，父进程把 `$NOPWD_BACKUP_DIR` 解析为绝对路径
    并以显式 `--backup-dir` 旗标传给提权后的子进程，环境变量无需额外配置即生效。
  - **手动 `sudo nopwd …` 时 shell 环境变量必丢**（sudo `env_reset`，无法恢复），
    此时配置文件生效（sudo 下读发起用户 home 的 `~/.nopwd.conf`）；若四级都
    未命中会给出黄色提示。**建议养成不手动加 sudo 的习惯**——工具会自动提权。

## 备份管理

`nopwd backup` 提供跨盘总览、校验、策略清理与手动删除，全部只访问备份目录，
**不会自动 sudo，也不会碰 `/dev/disk*` / `/dev/rdisk*`**：

```bash
nopwd backup [list] [--onlyid ID] [--backup-dir D]
nopwd backup verify [<备份.bin>] [--onlyid ID] [--index N] [--backup-dir D]
nopwd backup prune  [--onlyid ID] [--keep N] [--yes] [--backup-dir D]
nopwd backup rm     --onlyid ID [编号|范围]... [--yes] [--backup-dir D]
nopwd backup rm     <路径|文件名>... [--yes] [--backup-dir D]
```

- `list`：按物理盘分组显示全部 `.bin`；优先以 `onlyid` 分组，历史文件缺 onlyid
  时回退 `(device_id, 总扇区数)` 并标记为未知盘。每份备份同时检查固定大小
  `14 × 512 = 7168B` 与 `.md5` sidecar；正常为绿色 `MD5 ✓`，摘要损坏为红色，
  缺 sidecar 为黄色。每盘内部按新→旧编号 `[1] [2] ...`，下一行始终显示真实
  文件名；`--onlyid ID` 只显示指定物理盘。无法解析为本工具命名的 `.bin` 仍以
  灰色“未识别”列出。
- `verify`：无参数校验目录内全部 `.bin`，`--onlyid ID` 只校验指定盘，继续加
  `--index N` 可只校验该盘第 N 份；带文件名/
  路径时只校验该份（单文件与 `--onlyid` 不能同时使用）。大小不符、
  MD5 不符、缺 `.md5`、文件不存在均返回退出码 5；全部正常返回 0。
- 备份“新→旧”顺序以文件名中的 `_YYYYMMDD_HHMMSS` 创建时间为准；复制、`touch`
  等导致的文件系统 mtime 变化不会改变 `[1][2]...` 编号或 `prune` 的保留判断。
- 备份归属盘以文件内容 LBA4 的 onlyid 为权威；即使文件名被手工改错 onlyid，
  `list / inspect / rm / completion` 仍按真实 LBA4 归组。`.md5` 同时兼容纯摘要和
  标准 `HASH  filename` 格式，`verify` 与 `restore` 使用同一解析规则。
- 自动备份文件名中的 `device_id` 在落盘前执行严格安全字符校验；硬件返回的
  vendor/product 字符串若包含 `/`、控制字符或异常长度会直接拒绝创建备份，避免
  通过文件名路径分隔符写出备份目录。
- 自动备份输入必须恰好为 LBA0-13 共 7168B；文件名中的 onlyid 始终从这份备份
  自身的 LBA4 重新解析，不信任调用方缓存的身份字段。自动扫描/还原候选只接受
  备份目录中的普通 `.bin` 文件，符号链接及其它扩展名不会进入候选。
- `prune`：默认**只预览、不删除**；只有显式 `--yes` 才执行。加密原盘备份永不
  自动删除；每盘免密状态快照默认保留最新 2 份，可用 `--keep N` 调整，`--keep 0`
  允许清光免密快照，但前提是该盘仍有加密原盘备份；加 `--onlyid ID` 时策略只
  作用于该盘。
- **安全底线**：任何可识别盘组都不允许被清到 0 份备份。若某盘没有加密原盘
  备份，则即使 `--keep 0` 也会强制保留最新 1 份免密快照；`backup rm` 手动删除
  同样执行这条保护，不能把该盘最后一份备份删掉。
- `rm`：人工操作优先用 `--onlyid ID`。不给编号时先展示该盘新→旧列表并进入选择器，
  支持 `2`、`1,3`、`2-4`；直接给编号/范围时按当前列表解析。确认页再次显示编号、
  时间、类型、健康状态和真实文件名，并提示删除后还剩几份。`--yes` 只有在已经
  显式给出编号/范围时才允许使用，避免无选择目标的非交互误删。原有文件名/路径
  精确删除仍保留：裸文件名按当前备份目录解析，绝对/相对路径也必须最终落在当前
  备份目录内，否则拒绝。删除时 `.bin` 与对应 `.md5` 同步处理。
- 文件即使是 root 属主，只要备份目录本身对当前用户可写，仍可由普通用户删除；
  若目录由 root 持有且不可写，命令返回退出码 5，并明确提示检查目录属主/权限，
  必要时再手动使用 `sudo rm`。`nopwd backup` 自身不会提权。

## 扇区检查器

`nopwd inspect` 将原 `analyze/scripts/read_metadata.py` 的核心能力整合进正式 CLI，
但不照搬原脚本的大段无差别 hex 输出。物理盘与备份文件共用同一套解析器：

```bash
nopwd inspect                              # 当前 USB 盘；无盘时列出可查看的备份盘
nopwd inspect 0 4 6 7 8 9 11 12 --disk 14
nopwd inspect 7 12 --disk 14 --hex        # 解码后高亮 hex
nopwd inspect 7 --disk 14 --raw           # 只看盘上原始密文/原始字节
nopwd inspect --onlyid 1987718388          # 先列出 [1][2]...，不报用法错误
nopwd inspect --onlyid 1987718388 --index 2
nopwd inspect backup.bin 7 12 --hex        # --backup 可省略
nopwd inspect 6 7 11 12 --onlyid 1987718388 --index 2 --hex
nopwd inspect 7 12 --backup backup.bin --id 'disk&ven_...' --hex
nopwd inspect 6 7 12 --backup backup.bin --export ./metadata-out
```

- **来源统一**：不指定备份来源时优先查看物理 USB 盘；若当前没有外接 USB 盘，则直接
  列出备份目录中可用的 `onlyid`、型号、份数和最新时间，引导继续离线查看，而不是只报
  “未检测到外接盘”。缺 `--disk` 时会复用现有 USB 盘选择器；
  该路径仅做 `pread`/只读打开，不卸载、不写盘。`--backup` 支持任意备份/镜像路径，
  裸文件名按备份目录解析，也可直接写成位置参数 `nopwd inspect backup.bin ...`；
  `--onlyid ID` 单独使用时先展示可选备份，`--onlyid ID --index N` 与 `backup list` 的
  `[N]` 编号完全一致。
- **默认先看概览**：未指定 LBA 且未要求 `--hex/--raw` 时只扫描 LBA0-13，显示每扇区非零字节数、前导字符与
  已知解密方式，不直接打印 14×512B。指定 LBA 后显示结构化字段；无已知字段的扇区
  会自动退化为 hex。若明确加了 `--hex` 或 `--raw` 却没写 LBA，则按直觉展开全部 LBA0-13，
  不会悄悄忽略旗标；`--hex` 与 `--raw` 不能同时使用。
- **字段感知 hex**：`--hex` 显示解码后的 16B/行 hex，并按语义给已知字段着色：
  魔数/签名、文本、身份/密钥、地址/LBA、大小、类型/标志、校验分别使用不同语义色；
  校验失败使用红色。`--raw` 改看盘上原始 512B，不对密文套用解码字段颜色。
- **结构解析**：LBA0 MBR；LBA4 labelOnlyId；LBA6 SAFE6（GBK 标签/用户、CRC、注册标志、
  校验和）；LBA7 EDPF 64B entry；LBA8 LLGB；LBA9 SAPF；LBA11 DRKB/PDKB；
  LBA12 EDPF 96B entry。未知 LBA 保持 RAW。
- **备份比旧脚本更完整**：现代备份文件名本身已有 `device_id / VID / PID / 容量 / onlyid`，
  检查器会直接使用这些元数据，所以 LBA11 的 PDKB **在备份文件上也可解密**，不再只限
  当前硬件盘。旧/任意镜像若缺 device_id，可用 `--id` 手动补充。
- **LBA12 按已验证真实格式显示**：只对前 368B 做 A6B0 解密，尾部 144B 保持原始字节；
  不沿用旧脚本把整扇区都作为 AES 数据展示的方式。
- **导出**：`--export DIR` 为所查看扇区同时写出 `_raw.bin/.hex` 与
  `_decoded.bin/.hex`；未指定 LBA 时导出 LBA0-13 全部。hex 导出始终无 ANSI 色码。

### 从 v2（Python 版）迁移

| v2 | v3 |
|---|---|
| `sudo python3 -m nopwd --list` | `nopwd list` |
| `sudo python3 -m nopwd [--disk N]` | `nopwd run [--disk N]` |
| `sudo python3 -m nopwd --apply --force` | `nopwd apply --force` |
| `--restore` → 复制路径 → `--restore <bin> --apply` 三步 | `nopwd restore` 一条命令交互完成 |
| `make apply FORCE=1`（make 吃 flag 的坑已消失） | Makefile 已移除 |

### 退出码（脚本可区分失败类型）

| 码 | 含义 |
|---|---|
| 0 | 成功（含 dry-run/预览） |
| 1 | 运行时 IO 错误 |
| 2 | 用法错误（未知旗标/缺参数/参数非法） |
| 3 | 目标不可用（非 cems 盘/识别失败/size 越界/系统盘） |
| 4 | 已免密盘拒绝重复写入（需 `--force`） |
| 5 | 备份问题（无匹配/大小或 MD5 异常/缺 sidecar/删除失败/安全保护拒绝） |
| 6 | 写失败且回滚失败（中间态，需人工处理） |
| 7 | 写失败但已完整回滚（可安全重试） |
| 130 | 用户取消 |

## 代码结构

```
src/
  common.rs    公共常量、容量显示、Python 兼容舍入(银行家)、退出码契约
  crypto.rs    逆向 cemsusbregsiter.dll / sectormanage64.dll 得到的加密原语
  sectors.rs   扇区格式与转换（MBR / SAFE6 / EDPF），convert() 主编排
  identify.rs  device_id 识别（ioreg INQUIRY + 传输模式，LBA7 magic 判真）
  sysinfo.rs   diskutil/ioreg 查询（CmdRunner 抽象，测试注入罐头输出）
  diskio.rs    扇区设备抽象(SectorDev)、原子写入、备份/还原、备份元数据扫描/清理策略、快照读取
  inspect.rs   只读扇区解密/结构解析/字段感知 hex 渲染（物理盘与备份共用）
  completion.rs zsh/bash/fish 补全脚本 + onlyid/编号/盘号等动态候选
  md5.rs       MD5（备份 sidecar）
  plist.rs     极简 XML plist 解析（diskutil -plist 输出）
  elevate.rs   自动提权（sudo 重执行自身）
  cli.rs       子命令解析与各处理器（Ctx 注入，进程内可测）
tests/         集成测试（cargo test；金标 + 原子写三态 + 备份体系 + CLI）
backup/        真实盘备份（兼测试夹具，提交入库）
```

分层无环：`common → crypto → sectors / diskio → identify → cli`。
测试以**单文件版对真实盘备份的实测输出为金标**（三种型号 × 默认尺寸 + `--size`
路径的输出扇区 md5、CRC/K0、布局参数），锁死重构的行为零漂移 —— v2→v3 重写为
Rust 时即以此验证**字节级零漂移**（差分对齐：双实现离线产物逐字节 cmp 全一致，
含 `--size 1/1.5/2/7.5/10/32/50/50.5/60/63.9` 舍入矩阵）。另覆盖原子写入三态
（成功 / 中途失败自动回滚 / 读回不符回滚）、备份命名迁移、同型号他盘剔除
（LBA4 终验）、CLI 端到端。真实备份缺位时相关用例自动跳过。

CI 在 macOS 上固定执行 `cargo test --all-targets`、`cargo clippy --all-targets -- -D warnings`
和 release 构建；另用 Rust 1.75.0 执行 `cargo check --all-targets`，确保 `rust-version = "1.75"`
的最低版本承诺持续成立。

device_id 自动识别（SCSI INQUIRY + 传输模式 → Windows InstanceId 中间段，
两个候选用 LBA7 解出 EDPF magic 判真），无需手工输入。

备份匹配（`nopwd restore`）按 总扇区+VID/PID+device_id 分层匹配，并以
**LBA4 labelOnlyId（每盘随机唯一）终验** —— 同型号多块盘（device_id/容量
全同）也不会拿错备份。备份文件名含显式 `onlyid<labelOnlyId>` 段，人眼即可区分：
`disk{N}_{扇区数}_vid{}_pid{}_{device_id}_onlyid{labelOnlyId}[_nopwd]_{时间戳}.bin`。
工具在创建新备份或扫描还原备份时，也会读取历史 `.bin` 自身的 LBA4，自动把旧
`_lid..._` 或缺少 onlyid 的文件名迁移为 `_onlyid..._`，并同步重命名 `.md5`。

## 改造内容（5 个扇区，其余一律不动）

| 扇区 | 改动 |
|---|---|
| LBA0 | MBR 分区1 → type=0x07 @63 × Share 扇数（数据区直挂） |
| LBA6 | 0x1CA=128,480；0x1D4-0x1ED 清零；身份保留；校验和重算 |
| LBA7 | EDPF 2 条版本 2：Share@63 + 原 type4 指针原样；**表尾终止符@0xC0 保留** |
| LBA12 | EDPF 2 条版本 2：Share@63 + Encrypt(原盘真实位置)；**终止符@0x120 与尾部 144B 保留** |
| LBA9 | 非零则清零（EETU） |

三条铁律：EDPF 表尾终止符不清零；LBA12 尾部 144B 不清零；除必要字段外不发明
原盘没有的状态。分区参数全部按实际物理盘计算（Encrypt 取自原盘 LBA12 type=4）。

## 原子写入（全有或全无）

USB 盘硬件不提供跨扇区事务，`apply` / `restore` 的写入按七层逼近原子语义：

1. **重开前后状态终验** — 写前备份并确认后，必须先成功卸载；重新以 O_RDWR 打开
   `/dev/rdiskN` 后，再读 LBA0-13 与刚备份的写前快照逐扇比对。换盘、重枚举或同盘
   元数据在确认期间发生变化都在第一笔写入前拒绝；
2. **目标类型终验** — 真盘 `run / apply / restore / inspect --disk` 即使显式传了
   `--disk N`，也必须由 `diskutil` 确认为外接、非虚拟、WholeDisk 且 BusProtocol=USB；
   `disk2+` 只作为最低盘号保护，不再被视为“外接 U 盘”的充分条件；
3. **单 fd 全程持有** — 打开一次 `/dev/rdiskN` 直到全部写完、校验完，不再逐扇
   重开（旧版中途重开会撞 EBUSY，留下半写状态）；
4. **LBA0 最后写** — 唯一改 MBR 的扇区，写它才触发 macOS 重扫/挂载；
5. **介质缓存同步** — 每轮写入后先调用 `sync()`；真实 `/dev/rdiskN` 使用 macOS
   `DKIOCSYNCHRONIZECACHE`，普通镜像文件使用 `sync_all()`，同步成功后才进入读回；
   事务开始前还会先做一次同步能力预检，不支持该屏障的设备在第一笔写入前拒绝；
6. **逐扇读回校验** — 缓存同步完成后逐扇读回比对，避免只验证到内核写缓存；
7. **失败自动回滚** — 任一步失败，用写前内存镜像回滚全部扇区并再次同步、校验。
   回滚成功 = 盘仍为原状可安全重试（退出码 7）；回滚失败 = 明确报告中间态并
   指引 `nopwd restore` 从备份文件还原（写前已先落盘一份备份）（退出码 6）。

事务入口只接受 LBA0-13 且每项必须恰好 512B；越界 LBA 或非整扇区数据在第一笔
写入前直接拒绝。每次实际写入仍检查短写（`pwrite` 循环写满，0 视为失败）。

## 重复 apply 的行为（幂等 + 防误操作）

对已改造的免密盘再次 `apply`：转换是幂等的（四个扇区产物与首次逐字节一致，
实测三种型号），重写无害 —— 但工具默认**拒绝**：

- **检测**：LBA6（0x1CA 模板值，部分型号无区分度）/ MBR（分区1 type=0x07@63）/
  LBA12（解密后 entry0=Share@63+enc=1，entry1=Encrypt 指针 active=1，entry2 已清零）
  三处信号须同时成立。主信号是 LBA12 表结构：加密原盘恒为 3 条 EDPF
  （aigo 原盘 entry0 也是 type=2@63，故不能只看 type/start）。
- 拒绝时提示加 `--force`（退出码 4）；强制重写时自动备份的文件名含
  `_nopwd_` 段，`restore` 列表中该项标注 `[免密状态]` —— 还原它不会回到
  加密原盘，加密原盘备份是更早时间戳那份。备份打标按**内容**检测（与文件名无关）。

## 内置加密算法（逆向 cemsusbregsiter.dll / sectormanage64.dll）

- CRC32 bare：init=0、poly 0xEDB88320、无 final-xor
- LBA7：K0 = low16^high16(CRC32(device_id))，16 位字滚动 XOR
- LBA12：key = CRC32×4 ^ "EDPSECDISK200709"，AES-128 变体（counter=块号×16），仅前 368B
- LBA6：固定 K0=0x4DAA 滚动 XOR；校验和 = 密文 CRC32(bare) ×10 轮 ((v>>15)+(v<<1))

## 实测记录

2026-08-27 内网实测免密成功：aigo U335 128G / aigo U320 32G / Kingston DT3.0 64G。
每盘改前自动备份，`restore` 可完整还原。

v3（Rust 重写，2026-09-17）：金标零漂移验证 + 差分对齐（双实现产物逐字节一致）
后替换 v2；v2 代码见 git 标签 `python-final`。

2026-09-17 v3 真机验证（aigo U335 128G, disk26）全链路通过：`run` 识别/预览 →
`restore` 原盘备份还原（dd 14 扇区与备份逐字节一致）→ `apply` 重新转换（真盘
LBA0/6/7/9/12 md5 与金标逐项一致）→ 三信号检出免密盘 → 重复 `apply` 拒绝
（exit 4）→ `--force` 重写幂等 + 备份正确打 `_nopwd` 标。
