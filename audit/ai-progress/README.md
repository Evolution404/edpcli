# WebCodex 实时进展日志

本目录用于保存通过 WebCodex 连接本项目时的会话级实时进展日志。

## 强制约定

- 每个新的 WebCodex 工作会话必须新建一个独立 `.log` 文件，不得复用上一次会话日志。
- 文件名格式：`YYYYMMDD-HHMMSS-<wc_sess_id>.log`；若拿不到会话 ID，则使用 `YYYYMMDD-HHMMSS-manual.log`。
- 日志必须在开始实际分析、修改、测试或发布前创建。
- 每完成一个有意义的动作或得到一个可验证的新结论，都应立即追加一行；不要等到最终汇报时批量补写。
- 日志只记录事实、当前动作、验证结果和下一步，不记录隐藏推理过程。
- 时间使用本机本地时间，并带时区缩写。
- 推荐标签：`[START]`、`[WORK]`、`[FOUND]`、`[DECISION]`、`[PASS]`、`[FAIL]`、`[VALIDATION]`、`[GIT]`、`[REVIEW]`、`[NEXT]`、`[BLOCKED]`、`[DONE]`。

## 行格式

```text
[YYYY-MM-DD HH:MM:SS TZ] [TAG] 简洁、可验证的进展描述
```

示例：

```text
[2026-09-24 16:31:45 CST] [START] 追踪“取消密码复杂性验证”官方制盘 UI -> 协议字段 -> 消费端完整链路
[2026-09-24 16:32:11 CST] [FOUND] 官方制盘 UI 二进制中定位 pwdComplexityCheckBox
[2026-09-24 16:50:09 CST] [VALIDATION] cargo check --all-targets + git diff --check PASS
```

用户可通过：

```bash
tail -f audit/ai-progress/<当前会话日志>.log
```

实时查看进展。

`.log` 文件属于运行时会话记录，不纳入 Git；本目录的规范文件纳入 Git。
