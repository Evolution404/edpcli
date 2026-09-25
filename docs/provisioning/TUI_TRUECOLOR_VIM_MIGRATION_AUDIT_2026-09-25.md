# TUI TrueColor / Vim 迁移基线（2026-09-25）

本文件记录 `docs/provisioning/PROVISIONING.md` 第 10 章 Phase T0 的实际基线与迁移表。第 10 章仍是设计事实源；本文件只记录实施前现状、冲突和迁移落点。

## 基线

- 分支：`main`
- 起始 HEAD：`d5f518a1b289571ed2e6efcda9a6f4ca818c901a`
- `cargo test --test tui_suite`：132 passed / 0 failed
- `src/tui/**` 直接 `Color::*`：40 处
- `src/tui/**` 中 `KeyCode::Char/Tab/BackTab/Left/Right/PageUp/PageDown/Home/End`：107 处
- 工作区开始时已有 11 个未提交文件；这些修改属于既有工作，实施中必须保留，不得 reset/clean 或覆盖。

## 视觉冲突清单

| 现状 | 新语义 | 迁移 |
| --- | --- | --- |
| `render.rs` 自有 `ThemeToken` + ANSI 基础色 | 全局 Theme 单一事实源 | 迁移到 `src/tui/theme.rs` |
| Selection = Black on Cyan | 低亮度 selection + Accent 标记 | Theme selection token |
| Provision bar 直接 LightBlue/Cyan/Green/Magenta/Yellow | 六种低饱和 partition token | Theme partition token |
| animation 自有 Green/Yellow/Red/Cyan/Magenta | Theme animation token | 删除私有 palette |
| workspace render 可直接使用高饱和 `Color::*` | workspace 只使用语义 style | 增加静态门禁 |

## 键位迁移表

| 现有键 | 当前语义 | 新键/新语义 |
| --- | --- | --- |
| `Tab` / `Shift-Tab` | Workspace 前后切换 | Panel Next / Previous |
| `Left/Right`（普通页面） | Workspace 切换 | `h/l` 仅局部导航；Workspace 改 `gt/gT` |
| `h/l`（部分页面） | Workspace 切换 | List/Tree/Form/Hex 左右语义 |
| `g`（Inspect） | Jump | `gl` |
| `g g` | Top | 保留 |
| 无 | 下/上一个 Workspace | `gt/gT` |
| 无 | Devices/Backups/Provision/Inspect | `gd/gb/gp/gi` |
| 无 | Panel 左/下/上/右/下一个/上一个 | `Ctrl-w h/j/k/l/w/W` |
| `i/I` | 打开 Inspect | `gi`；`i` 回归 Insert |
| `D` | 单条 Backup 删除 | `d` + Confirm |
| `X` | 批量 Backup 删除 | `d` 根据 selection 决定 + Confirm |
| `r/d/m`（Sector） | Raw/Decode/Mixed | `v` 循环 |
| `r` | Refresh（多数页面） | 全局 Refresh |
| `d` | 页面局部语义 | 全局 Delete，必须 Confirm |
| `/` | Search | 保留，进入 Search mode |
| `:` | Command | 保留，进入 Command mode |
| 文本字段直接接收字符 | 表单编辑 | Normal 下 `i/Enter` 进入 Insert；Insert 字母不触发命令 |

## 历史测试处理原则

- 不删除已有 shortcut 测试来规避冲突。
- 旧测试若锁定被第 10 章明确废止的映射，改为断言新映射并新增“旧映射不再生效”的反向门禁。
- 协议解析、ProvisionRequest、事务写入、安全 guard 测试不得因 TUI 重构而弱化。
- 典型终端尺寸继续覆盖 40×10、60×18、80×24、120×36；新增 Theme/keymap 契约测试。
