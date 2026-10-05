use super::{help::HelpBinding, TuiAction};

pub const PROVISION_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "Tab / Shift-Tab",
        label: "切换制盘配置窗口",
        action: TuiAction::FocusNext,
    },
    HelpBinding {
        keys: "Ctrl-w h/j/k/l · w/W",
        label: "按方向 / 顺序切换窗口",
        action: TuiAction::PanelNext,
    },
    HelpBinding {
        keys: "j/k · ↑/↓",
        label: "选择字段 / 当前窗口内移动",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "i",
        label: "编辑当前字段",
        action: TuiAction::Insert,
    },
    HelpBinding {
        keys: "h / l",
        label: "当前选项上一个 / 下一个",
        action: TuiAction::MoveRight,
    },
    HelpBinding {
        keys: "Space",
        label: "切换当前选项 / 容量单位 / 新密码透传",
        action: TuiAction::Toggle,
    },
    HelpBinding {
        keys: "f",
        label: "起点/容量自动求解可用空间",
        action: TuiAction::Fill,
    },
    HelpBinding {
        keys: "Enter",
        label: "进入下一阶段",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "e",
        label: "计划页导出镜像",
        action: TuiAction::Export,
    },
];

pub const PROVISION_REVIEW_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓ · PgUp/PgDn",
        label: "当前窗口内选择 / 滚动",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "o",
        label: "展开 / 折叠当前详情",
        action: TuiAction::Open,
    },
    HelpBinding {
        keys: "Tab / Shift-Tab",
        label: "切换计划确认窗口",
        action: TuiAction::FocusNext,
    },
    HelpBinding {
        keys: "Ctrl-w h/j/k/l · w/W",
        label: "按方向 / 顺序切换窗口",
        action: TuiAction::PanelNext,
    },
    HelpBinding {
        keys: "Enter",
        label: "进入写入确认",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "e",
        label: "导出当前制盘镜像",
        action: TuiAction::Export,
    },
    HelpBinding {
        keys: "Esc",
        label: "返回制盘配置",
        action: TuiAction::Back,
    },
];

pub const PROVISION_RUNNING_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓ · PgUp/PgDn",
        label: "浏览运行记录",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "G",
        label: "返回最新运行记录并继续跟随",
        action: TuiAction::Bottom,
    },
];

pub const PROVISION_RESULT_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓ · PgUp/PgDn",
        label: "当前窗口内选择 / 滚动",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "Tab / Shift-Tab",
        label: "切换结果窗口",
        action: TuiAction::FocusNext,
    },
    HelpBinding {
        keys: "Ctrl-w h/j/k/l · w/W",
        label: "按方向 / 顺序切换结果窗口",
        action: TuiAction::PanelNext,
    },
    HelpBinding {
        keys: "Enter / Esc",
        label: "返回设备列表",
        action: TuiAction::Back,
    },
];

pub const RESTORE_RESULT_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓ · PgUp/PgDn",
        label: "当前窗口内选择 / 滚动",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "Tab / Shift-Tab",
        label: "切换结果窗口",
        action: TuiAction::FocusNext,
    },
    HelpBinding {
        keys: "Ctrl-w h/j/k/l · w/W",
        label: "按方向 / 顺序切换结果窗口",
        action: TuiAction::PanelNext,
    },
    HelpBinding {
        keys: "Enter",
        label: "处理当前分区（仅分区结果窗口）",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "Esc",
        label: "完成并返回",
        action: TuiAction::Back,
    },
];

pub const BUSY_GLOBAL_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "Esc",
        label: "当前不可返回；显示处理进度",
        action: TuiAction::Back,
    },
    HelpBinding {
        keys: "q / Ctrl-C",
        label: "退出",
        action: TuiAction::Quit,
    },
    HelpBinding {
        keys: "?",
        label: "打开 / 关闭帮助",
        action: TuiAction::Help,
    },
];

pub const GUARD_GLOBAL_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "Esc",
        label: "当前安全阶段不可返回",
        action: TuiAction::Back,
    },
    HelpBinding {
        keys: "q / Ctrl-C",
        label: "请求退出；在安全结束点处理",
        action: TuiAction::Quit,
    },
    HelpBinding {
        keys: "?",
        label: "打开 / 关闭帮助",
        action: TuiAction::Help,
    },
];

pub const PICKER_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓",
        label: "切换方案",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "Enter",
        label: "确认选择",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "Esc",
        label: "取消并关闭",
        action: TuiAction::Back,
    },
];
