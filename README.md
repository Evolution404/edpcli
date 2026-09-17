# nopwd_tool — cems 加密 U 盘 → 无密码盘

纯 Rust 标准库实现，**零外部依赖**，单二进制 `nopwd`（macOS）。

## 快速使用

```bash
cargo build --release            # 或 cargo install --path . 装入 ~/.cargo/bin
./target/release/nopwd list      # 列出外接盘：编号/容量/接口/cems 识别/备份份数（免 sudo）
./target/release/nopwd run       # 预览改造（dry-run；缺省子命令，裸 nopwd 同义；自动检测 USB 盘）
nopwd run --disk 4               # 指定盘（接受 4 / /dev/disk4 / /dev/rdisk4）
nopwd apply                      # 实际写入（自动备份 → 原子写入 → 读回校验）
nopwd apply --disk 4 --size 100  # 指定盘 + Share 100GB
nopwd apply --force              # 盘已是免密盘仍强制重写（默认拒绝）
nopwd restore                    # 交互还原：列出本盘备份（新→旧，免密快照标注）→ 选择 → YES → 写入
nopwd restore <备份.bin> --yes   # 脚本化还原（自动确认）
nopwd convert --dir <快照目录> --id <device_id> [--out <目录>]   # 离线验证（不碰真盘）
```

- **自动提权**：`run` / `apply` / `restore` 需要裸盘读写，非 root 时自动以 `sudo`
  重执行自身（选定盘号并入参数，交互提示正常工作）；`list` / `convert` 永不提权。
- `--yes` 免交互；多块 USB 盘时自动弹编号选择；系统盘（disk<2）一律拒绝。
- 备份目录：`--backup-dir` > 环境变量 `NOPWD_BACKUP_DIR` > `./backup`。

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
| 5 | 备份问题（无匹配/大小不符/MD5 不符） |
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
  diskio.rs    扇区设备抽象(SectorDev)、原子写入、备份/还原、快照读取
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

USB 盘硬件不提供跨扇区事务，`apply` / `restore` 的写入按四层逼近原子语义：

1. **单 fd 全程持有** — 打开一次 `/dev/rdiskN` 直到全部写完、校验完，不再逐扇
   重开（旧版中途重开会撞 EBUSY，留下半写状态）；
2. **LBA0 最后写** — 唯一改 MBR 的扇区，写它才触发 macOS 重扫/挂载；
3. **逐扇读回校验** — 全部写完后读回比对，落盘与否以读回为准；
4. **失败自动回滚** — 任一步失败，用写前内存镜像回滚全部扇区并再校验。
   回滚成功 = 盘仍为原状可安全重试（退出码 7）；回滚失败 = 明确报告中间态并
   指引 `nopwd restore` 从备份文件还原（写前已先落盘一份备份）（退出码 6）。

每次写入均检查短写（`pwrite` 循环写满，0 视为失败）。

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
