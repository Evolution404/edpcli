use super::TuiAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpBinding {
    pub keys: &'static str,
    pub label: &'static str,
    pub action: TuiAction,
}

pub const INSPECT_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "Tab/Shift-Tab",
        label: "切换当前页 Pane",
        action: TuiAction::FocusNext,
    },
    HelpBinding {
        keys: "j/k",
        label: "Move",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "h/l",
        label: "Fold",
        action: TuiAction::MoveLeft,
    },
    HelpBinding {
        keys: "字段表 h/l · </> · H/L · 0/$ · s/S · y/Y",
        label: "列 · 视口 · 排序 · 复制",
        action: TuiAction::TableColumnRight,
    },
    HelpBinding {
        keys: "Enter",
        label: "Open",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "o",
        label: "Toggle",
        action: TuiAction::Open,
    },
    HelpBinding {
        keys: "/",
        label: "Search",
        action: TuiAction::Search,
    },
    HelpBinding {
        keys: "n/N",
        label: "Match",
        action: TuiAction::NextMatch,
    },
    HelpBinding {
        keys: "J",
        label: "跳转到 LBA",
        action: TuiAction::InspectJump,
    },
    HelpBinding {
        keys: "[/]",
        label: "Sector",
        action: TuiAction::SectorPrevious,
    },
    HelpBinding {
        keys: "?",
        label: "Help",
        action: TuiAction::Help,
    },
];

pub const DEVICES_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓",
        label: "选择设备 / 当前 Pane 内移动",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "Enter",
        label: "打开设备信息 / 进入详情",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "o",
        label: "展开 / 折叠结构树",
        action: TuiAction::Open,
    },
    HelpBinding {
        keys: "p",
        label: "选择制盘方案",
        action: TuiAction::Provision,
    },
    HelpBinding {
        keys: "i",
        label: "检查当前设备",
        action: TuiAction::Insert,
    },
    HelpBinding {
        keys: "b",
        label: "备份当前设备元数据",
        action: TuiAction::BackupCreate,
    },
    HelpBinding {
        keys: "R",
        label: "恢复详情中选中的备份",
        action: TuiAction::Restore,
    },
];

pub const BACKUPS_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓",
        label: "选择备份",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "H/L",
        label: "设备树 / 表格平滑横移",
        action: TuiAction::TableScrollRight,
    },
    HelpBinding {
        keys: "Space",
        label: "勾选 / 取消勾选",
        action: TuiAction::Toggle,
    },
    HelpBinding {
        keys: "Enter / i",
        label: "检查备份",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "b",
        label: "新建备份",
        action: TuiAction::BackupCreate,
    },
    HelpBinding {
        keys: "v",
        label: "校验备份",
        action: TuiAction::ViewOrVerify,
    },
    HelpBinding {
        keys: "R",
        label: "恢复备份",
        action: TuiAction::Restore,
    },
    HelpBinding {
        keys: "d",
        label: "删除备份",
        action: TuiAction::Delete,
    },
];

pub const PROVISION_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓",
        label: "选择字段 / 当前 Pane 内移动",
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

pub const GLOBAL_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "Tab / Shift-Tab",
        label: "切换当前层级焦点 / 顶层标签",
        action: TuiAction::FocusNext,
    },
    HelpBinding {
        keys: "Esc",
        label: "返回 / 取消当前层级",
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

pub const TABLE_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "h / l",
        label: "上一列 / 下一列",
        action: TuiAction::TableColumnRight,
    },
    HelpBinding {
        keys: "H / L",
        label: "横向滚动视口",
        action: TuiAction::TableScrollRight,
    },
    HelpBinding {
        keys: "< / >",
        label: "左移 / 右移当前整列",
        action: TuiAction::TableMoveColumnRight,
    },
    HelpBinding {
        keys: "0 / $",
        label: "第一列 / 最后一列",
        action: TuiAction::TableColumnFirst,
    },
    HelpBinding {
        keys: "s / S",
        label: "排序 / 恢复默认顺序",
        action: TuiAction::TableSortToggle,
    },
    HelpBinding {
        keys: "y / Y",
        label: "复制单元格 / 整行",
        action: TuiAction::TableCopyCell,
    },
];
