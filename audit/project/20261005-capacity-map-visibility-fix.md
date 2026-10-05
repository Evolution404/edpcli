# 容量图小区域可见性修复

2026-10-05。上一轮将所有色带都改成严格比例分配，小于一个终端单元格的区域获得零宽度，导致用户无法在图中辨认小区域。这是展示回归；只在区域表中保留条目不足以满足容量图的使用需求。

修复后的 Full 容量图包含两种明确区分的表达：

- 上方百分比轨道严格按真实容量分配，不放大小区域。
- 下方区域示意在空间足够时为每个区域保留至少 3 格，并标注“区域示意（小区域放大）”。选中 `▲` 与示意色块对齐。
- 区域表继续显示准确容量、占比及 LBA；Compact/Mini 也保留小区域色块，不附线性百分比刻度。

设备、备份、制盘审核、制盘结果及恢复结果共用该组件。Full 地图增加一行，相关窗口同步增加地图高度，避免裁掉选中标记或挤压下方字段。

## 验证

- 新增 98、158、238 格地图的回归：每个小区域色块不少于 3 格、总宽度正确、逐区域选中标记位置不同，选中样式正确；真实比例轨道仍通过小于一格的比例误差验证。
- `scripts/test-fast.sh` 通过：8 suites、10 artifacts、失败 0，14.36 秒。
- `python3 scripts/test-full.py --profile full --max-seconds 600` 通过：8 suites、10 artifacts，另含 doctest，失败 0，13.54 秒。
- 200×60 本机 PTY 回放：真实设备页、真实备份页、审核及成功结果页已目视检查，小区域色块恢复，区域表及 LBA 完整可见。备份演示夹具没有可靠布局，不将它作为地图验证证据。
- 五个自有回放会话正常退出；真实页面仅只读导航，未写入物理设备。图片为终端单元格重建图，不是 iTerm2 原生截图。

新版通过 `scripts/install-local.sh` 安装至 `/Users/zhangyuxi/.local/bin/edpcli`，交互式 zsh 路径确认正确。构建时间 `2026-10-05T21:46:31+08:00`，源/安装 SHA-256 同为 `1a39a7987113a438fad6f6544bf90d0f3a621f38fe3548ff9ec3cfc67c70f30c`。

[验收证据](/Users/zhangyuxi/.local/state/edpcli/audit/20261005-213908-capacity-fix)、[真实备份页效果](/Users/zhangyuxi/.local/state/edpcli/audit/20261005-213908-capacity-fix/frames/real-backups/color-screen-02.png)。
