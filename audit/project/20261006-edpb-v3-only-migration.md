# EDPB v3 收敛与本地历史备份迁移 · 2026-10-06

## 结果与范围

按用户最新要求删除 EDPB 历史兼容，同时继续保护 EDP 物理盘协议兼容。读取与写入只支持 `edpb.manifest.v3`；外层二进制容器仍为 1.0，块布局与校验算法不变。

- 删除 v1/v2 schema 分支、`LegacyMigrated` 枚举、`src/edpb/legacy.rs`、旧摘要授权备注解析、类型化硬件 `serial_sha256` 字段与历史身份投影回退。
- 删除备份身份匹配、恢复授权和自动备份分组的历史摘要回退；当前运行时提权/恢复 pin 的摘要传递继续使用，避免序列号原文进入提权参数。
- 删除历史夹具写入模块，使用现有 v3 容器的清单变异构造拒绝测试。清单变异重算头尾摘要并保留块负载，确保测试到达 schema/身份校验，而非仅校验外层损坏。
- v3 强制类型化身份、`metadata_only` 目的与恢复契约；拒绝旧 schema、旧 capture level、旧摘要字段/备注。旧格式恢复测试确认设备写入次数为零。
- TUI 示例和测试夹具使用当前原文身份；修复两处依赖旧 `backup/*.bin` 的测试入口。
- 一次性离线转换脚本保存在本地归档目录，未引入生产迁移器或新的兼容入口。

## 本地迁移证据

迁移前，实际配置目录 `~/.edpcli-backup` 有 97 份 EDPB，全部先通过原读取器校验。另有用户目录与仓库目录各 23 份 6656B 原始快照及 SHA-256 旁挂文件。

| 类别 | 数量 | 处理 |
| --- | ---: | --- |
| EDPB v1 | 27 | 转成 v3，原件归档 |
| EDPB v2 | 7 | 转成 v3，原件归档 |
| 已是 v3 | 63 | 原文件 SHA-256 不变 |
| 原始快照 | 46 | 22 份与已有 EDPB 完全重复；另 24 份生成 v3 |
| 最终活动 EDPB | 121 | 用户目录 98，仓库目录 23；新版读取器全部校验通过 |
| 已归档原文件 | 126 | 34 份旧 EDPB、46 份 `.bin` 与 46 份 `.sha256`，逐份校验 SHA-256 |

本地归档位置：`~/.edpcli-backup-upgrade/20261006-115539/originals/`。同级 `plan.json`、`strict-staged-verification.json`、`final-verification.json`、`applied.json` 与一次性脚本记录转换计划、工件摘要和执行结果；目录权限收敛为仅当前用户访问。

每份旧 EDPB 在活动目录替换前先复制并校验原件；新文件暂存通过严格 v3 读取器后才原子替换。校验失败的转换事务可恢复原文件。最终保留的原始协议、LBA7/LCE、已采集盘尾元数据及派生分区证据的 SHA-256 与源文件一致。历史非元数据工件、旧文件系统探测、旧空占位保存在原件归档中，不进入当前 `metadata_only` 清单。

8 份记录仅有序列号摘要，其中 5 份通过已有 v3 序列号的 SHA-256 与 VID/PID 精确匹配找回原文。其余 3 份（Teclast、Lexar、Kingston）明确标记 `missing`，保留摘要为迁移来源记录；不能凭此授权破坏性恢复。没有补造身份、LCE、盘尾或文件系统内容，没有打开实盘。

仓库 23 份快照转换为 v3 后纳入提交。22 份原始字节已存在于独立协议证据目录；唯一缺少的历史 AIGO mode1 样本单独保存为 `tests/fixtures/protocol/mode1/aigo_u335_20260828_lba0_12.bin`，带 SHA-256 与来源记录。现有协议证据、gold 总体与 `src/protocol/` 均未修改。

## 验证

- `scripts/test-fast.sh`：格式/差异检查、Clippy `-D warnings`、table-scroll 门禁及 8 套件 / 10 构建产物通过，0 失败，runner 29.11 秒。
- `python3 scripts/protocol/audit_baseline.py`：20 份指定协议样本通过，包括 19 份 strict-encrypted 与 1 份 authentic-mode1。
- 本地转换：58 份暂存候选与最终 121 份活动容器全部通过 v3-only 读取器；工件字节一致，63 份原 v3 文件不变，126 份原件归档摘要一致。
- `python3 scripts/test-full.py --profile full --deadline-seconds 900 --max-seconds 900`：8 套件 / 10 构建产物与 doctest 通过，0 失败，18.36 秒。
- Windows `x86_64-pc-windows-gnu` 与 Linux `x86_64-unknown-linux-gnu` 的 `cargo check --locked --all-targets` 均通过。
- 当前构建的 CLI 对用户目录 98 份、仓库目录 23 份备份执行 `backup verify --backup-dir ...`，均成功退出并输出校验通过。
- 最终复核 126 份归档摘要；升级前后协议身份、派生候选、硬件型号及几何不变。`src/protocol/`、`src/provision/`、`src/domain/`、`src/platform/`、`audit/protocol/` 与已有协议证据均无差异。
- 未运行虚拟或实盘 HIL。

上一轮开发兼容清理已提交推送为 `71d8b3d`。本报告与本轮代码、仓库备份转换一并提交到主分支；实际提交与推送结果记入会话进度日志。未修改本机已安装二进制或创建正式版本发布。
