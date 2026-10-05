use super::TuiAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpBinding {
    pub keys: &'static str,
    pub label: &'static str,
    pub action: TuiAction,
}

pub const INSPECT_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "Tab / Shift-Tab",
        label: "切换当前页窗口",
        action: TuiAction::FocusNext,
    },
    HelpBinding {
        keys: "Ctrl-w h/j/k/l · w/W",
        label: "按方向 / 顺序切换窗口",
        action: TuiAction::PanelNext,
    },
    HelpBinding {
        keys: "j/k",
        label: "移动 / 选择",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "h/l",
        label: "结构树：折叠 / 展开",
        action: TuiAction::MoveLeft,
    },
    HelpBinding {
        keys: "字段表 h/l · </> · H/L · 0/$ · s/S · y/Y",
        label: "列 · 视口 · 排序 · 复制",
        action: TuiAction::TableColumnRight,
    },
    HelpBinding {
        keys: "Enter",
        label: "打开 / 进入",
        action: TuiAction::Activate,
    },
    HelpBinding {
        keys: "o",
        label: "结构树 / 字段：展开 / 折叠",
        action: TuiAction::Open,
    },
    HelpBinding {
        keys: "/",
        label: "搜索",
        action: TuiAction::Search,
    },
    HelpBinding {
        keys: "n/N",
        label: "下一个 / 上一个匹配",
        action: TuiAction::NextMatch,
    },
    HelpBinding {
        keys: "J",
        label: "跳转到 LBA",
        action: TuiAction::InspectJump,
    },
];

pub const SECTOR_INSPECT_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "h/l · ←/→",
        label: "上一 / 下一字节",
        action: TuiAction::MoveRight,
    },
    HelpBinding {
        keys: "j/k · ↑/↓",
        label: "下 / 上移动 16 字节",
        action: TuiAction::MoveDown,
    },
    HelpBinding {
        keys: "0 / $",
        label: "当前行首 / 行尾",
        action: TuiAction::RowStart,
    },
    HelpBinding {
        keys: "gg / G",
        label: "当前扇区首字节 / 末字节",
        action: TuiAction::Top,
    },
    HelpBinding {
        keys: "Ctrl-u / Ctrl-d",
        label: "上 / 下移动半页",
        action: TuiAction::HalfPageUp,
    },
    HelpBinding {
        keys: "PgUp / PgDn",
        label: "上 / 下移动一页",
        action: TuiAction::PageUp,
    },
    HelpBinding {
        keys: "[ / ]",
        label: "上一 / 下一扇区",
        action: TuiAction::SectorPrevious,
    },
    HelpBinding {
        keys: "v",
        label: "切换 Raw / Decode / Mixed",
        action: TuiAction::ViewOrVerify,
    },
    HelpBinding {
        keys: "o",
        label: "展开 / 折叠字段详情",
        action: TuiAction::Open,
    },
    HelpBinding {
        keys: "y / Y",
        label: "复制当前值 / 原始字节",
        action: TuiAction::Yank,
    },
    HelpBinding {
        keys: "/ · n/N",
        label: "搜索 · 下一个 / 上一个匹配",
        action: TuiAction::Search,
    },
    HelpBinding {
        keys: "J",
        label: "跳转到 LBA",
        action: TuiAction::InspectJump,
    },
    HelpBinding {
        keys: "Esc",
        label: "返回检查",
        action: TuiAction::Back,
    },
];

pub const DEVICES_HELP: &[HelpBinding] = &[
    HelpBinding {
        keys: "j/k · ↑/↓",
        label: "选择设备 / 当前窗口内移动",
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
        keys: "r",
        label: "刷新设备列表",
        action: TuiAction::Refresh,
    },
    HelpBinding {
        keys: "Ctrl-w h/j/k/l · w/W",
        label: "切换设备工作台窗口",
        action: TuiAction::PanelNext,
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
        keys: "Ctrl-w h/j/k/l · w/W",
        label: "切换备份工作台窗口",
        action: TuiAction::PanelNext,
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
