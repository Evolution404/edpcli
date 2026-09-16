# nopwd_tool — cems 加密 U 盘 → 无密码盘

仅依赖 Python 3 标准库，无任何外部依赖。

## 快速使用（Makefile）

```bash
make test                        # 测试套件（不碰真盘）
make list                        # 列出外接盘：编号/容量/接口/cems 识别/备份份数
make run                         # 预览改造（dry-run，自动检测 USB 盘）
make run DISK=4                  # 预览指定盘
make apply                       # 实际写入（自动备份 → 原子写入 → 读回校验）
make apply DISK=4 SIZE=100       # 指定盘 + Share 100GB
make apply FORCE=1               # 盘已是免密盘仍强制重写（默认拒绝；也可 make apply -- --force，
                                 #   注意 make apply --force 无效 —— --force 会被 make 当作自己的选项）
make restore                     # 列出本盘匹配的备份（唯一备份时命令直填路径）
make restore RESTORE=<备份.bin>  # 还原预检（MD5 校验 + 预览）
make restore RESTORE=<备份.bin> APPLY=1   # 还原写入
```

任意参数原样透传（`--` 之后的内容直接交给工具；不加 `--` 的话 `--disk` 会被
make 当成自己的选项吞掉）：

```bash
make run -- --disk 4 --size 100
```

等价的原始命令（`python3 -m nopwd`）：

```bash
sudo python3 -m nopwd                    # 自动检测 USB 盘，dry-run
sudo python3 -m nopwd --apply            # 实际写入（自动备份 LBA0-13 到 backup/）
sudo python3 -m nopwd --disk 4           # 也可手动指定盘号
sudo python3 -m nopwd --restore          # 列出本盘匹配的备份，不写入
sudo python3 -m nopwd --restore <备份.bin>          # 还原预检（MD5 校验 + 预览）
sudo python3 -m nopwd --restore <备份.bin> --apply  # 还原写入（YES 确认后执行）
python3 -m nopwd --list           # 列出外接盘（sudo 下可多显示 cems 识别/备份）
python3 -m nopwd --dir <快照目录> --id <device_id> [--out <输出目录>]  # 离线验证
```

可选参数：`--size 100`（Share 数据区大小 GB，默认占满到加密区之前）。

备份目录：环境变量 `NOPWD_BACKUP_DIR` 显式优先，缺省 `./backup`（相对当前目录）。
Makefile 已把它固定为仓库 `backup/`。

## 代码结构

```
nopwd/
  common.py    公共常量（SECTOR）与容量显示
  crypto.py    逆向 cemsusbregsiter.dll / sectormanage64.dll 得到的加密原语
  sectors.py   扇区格式与转换（MBR / SAFE6 / EDPF），convert() 主编排
  identify.py  device_id 识别（ioreg INQUIRY + 传输模式，LBA7 magic 判真）
  diskio.py    真盘 IO、原子写入、备份/还原、盘枚举
  cli.py       命令行入口（python3 -m nopwd）
tests/         unittest 测试套件（make test）
```

分层无环：`common → crypto → sectors / diskio → identify → cli`。
测试以**拆包前单文件版对真实盘备份的实测输出为金标**（三种型号 × 默认尺寸 +
`--size` 路径的输出扇区 md5、CRC/K0、布局参数），锁死重构的行为零漂移；
另覆盖原子写入三态（成功 / 中途失败自动回滚 / 读回不符回滚）、备份命名迁移、
同型号他盘剔除（LBA4 终验）、CLI 端到端。真实备份缺位时相关用例自动跳过。

device_id 自动识别（SCSI INQUIRY + 传输模式 → Windows InstanceId 中间段，
两个候选用 LBA7 解出 EDPF magic 判真），无需手工输入。

备份匹配（`--restore` 不带值）按 总扇区+VID/PID+device_id 分层匹配，并以
**LBA4 labelOnlyId（每盘随机唯一）终验** —— 同型号多块盘（device_id/容量
全同）也不会拿错备份。备份文件名含显式 `onlyid<labelOnlyId>` 段，人眼即可区分：
`disk{N}_{扇区数}_vid{}_pid{}_{device_id}_onlyid{labelOnlyId}_{时间戳}.bin`。
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

USB 盘硬件不提供跨扇区事务，`--apply` / `--restore` 的写入按四层逼近原子语义：

1. **单 fd 全程持有** — 打开一次 `/dev/rdiskN` 直到全部写完、校验完，不再逐扇
   重开（旧版中途重开会撞 EBUSY，留下半写状态）；
2. **LBA0 最后写** — 唯一改 MBR 的扇区，写它才触发 macOS 重扫/挂载；
3. **逐扇读回校验** — 全部写完后 `pread` 比对，落盘与否以读回为准；
4. **失败自动回滚** — 任一步失败，用写前内存镜像回滚全部扇区并再校验。
   回滚成功 = 盘仍为原状可安全重试；回滚失败 = 明确报告中间态并指引
   `--restore` 从备份文件还原（写前 `backup_disk` 已先落盘一份备份）。

每次写入均检查 `pwrite` 完整返回值（短写会静默丢数据）。

## 重复 --apply 的行为（幂等 + 防误操作）

对已改造的免密盘再次 `--apply`：转换是幂等的（四个扇区产物与首次逐字节一致，
实测三种型号），重写无害 —— 但工具默认**拒绝**：

- **检测**：LBA6（0x1CA 模板值，部分型号无区分度）/ MBR（分区1 type=0x07@63）/
  LBA12（解密后 entry0=Share@63+enc=1，entry1=Encrypt 指针 active=1，entry2 已清零）
  三处信号须同时成立。主信号是 LBA12 表结构：加密原盘恒为 3 条 EDPF
  （aigo 原盘 entry0 也是 type=2@63，故不能只看 type/start）。
- 拒绝时提示加 `--force`（make：`FORCE=1`）；强制重写时自动备份的文件名含
  `_nopwd_` 段，`--restore` 列表中该项标注 `[免密状态]` —— 还原它不会回到
  加密原盘，加密原盘备份是更早时间戳那份。备份打标按**内容**检测（与文件名无关）。

## 内置加密算法（逆向 cemsusbregsiter.dll / sectormanage64.dll）

- CRC32 bare：init=0、poly 0xEDB88320、无 final-xor
- LBA7：K0 = low16^high16(CRC32(device_id))，16 位字滚动 XOR
- LBA12：key = CRC32×4 ^ "EDPSECDISK200709"，AES-128 变体（counter=块号×16），仅前 368B
- LBA6：固定 K0=0x4DAA 滚动 XOR；校验和 = 密文 CRC32(bare) ×10 轮 ((v>>15)+(v<<1))

## 实测记录

2026-08-27 内网实测免密成功：aigo U335 128G / aigo U320 32G / Kingston DT3.0 64G。
每盘改前自动备份，`--restore` 可完整还原。
