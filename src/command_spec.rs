//! Public CLI grammar catalog shared by help and shell completion.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionSpec {
    pub name: &'static str,
    pub takes_value: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpec {
    pub name: &'static str,
    pub summary: &'static str,
    pub options: &'static [OptionSpec],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
    pub usage: &'static str,
    pub options: &'static [OptionSpec],
    pub actions: &'static [ActionSpec],
}

impl CommandSpec {
    pub fn action_names(&self) -> Vec<&'static str> {
        self.actions.iter().map(|action| action.name).collect()
    }

    pub fn option_names(&self, action: Option<&str>) -> Vec<&'static str> {
        let mut out: Vec<_> = self.options.iter().map(|option| option.name).collect();
        if let Some(action) = action {
            if let Some(spec) = self
                .actions
                .iter()
                .find(|candidate| candidate.name == action)
            {
                for option in spec.options {
                    if !out.contains(&option.name) {
                        out.push(option.name);
                    }
                }
            }
        }
        out
    }
}

const fn value(name: &'static str) -> OptionSpec {
    OptionSpec {
        name,
        takes_value: true,
    }
}

const fn switch(name: &'static str) -> OptionSpec {
    OptionSpec {
        name,
        takes_value: false,
    }
}

const EMPTY_OPTIONS: &[OptionSpec] = &[];
const HELP: OptionSpec = switch("--help");

const LIST_OPTIONS: &[OptionSpec] = &[value("--backup-dir"), HELP];
const INFO_OPTIONS: &[OptionSpec] = &[value("--disk"), value("--id"), value("--backup-dir"), HELP];
const INSPECT_OPTIONS: &[OptionSpec] = &[
    value("--disk"),
    value("--lba"),
    value("--count"),
    value("--export"),
    value("--id"),
    value("--backup-dir"),
    HELP,
];

const BACKUP_CREATE_OPTIONS: &[OptionSpec] = &[
    value("--disk"),
    switch("--deep"),
    value("--backup-dir"),
    HELP,
];
const BACKUP_LIST_OPTIONS: &[OptionSpec] = &[value("--backup-dir"), HELP];
const BACKUP_RESTORE_OPTIONS: &[OptionSpec] = &[
    value("--disk"),
    switch("--yes"),
    value("--backup-dir"),
    HELP,
];
const BACKUP_VERIFY_OPTIONS: &[OptionSpec] = &[value("--backup-dir"), HELP];
const BACKUP_DELETE_OPTIONS: &[OptionSpec] = &[switch("--yes"), value("--backup-dir"), HELP];
const BACKUP_PRUNE_OPTIONS: &[OptionSpec] = &[
    value("--keep"),
    switch("--yes"),
    value("--backup-dir"),
    HELP,
];

const BACKUP_ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "create",
        summary: "创建只读备份",
        options: BACKUP_CREATE_OPTIONS,
    },
    ActionSpec {
        name: "list",
        summary: "列出备份",
        options: BACKUP_LIST_OPTIONS,
    },
    ActionSpec {
        name: "restore",
        summary: "恢复备份",
        options: BACKUP_RESTORE_OPTIONS,
    },
    ActionSpec {
        name: "verify",
        summary: "校验备份",
        options: BACKUP_VERIFY_OPTIONS,
    },
    ActionSpec {
        name: "delete",
        summary: "删除备份",
        options: BACKUP_DELETE_OPTIONS,
    },
    ActionSpec {
        name: "prune",
        summary: "按保留数量清理备份",
        options: BACKUP_PRUNE_OPTIONS,
    },
];

const PROVISION_COMMON_OPTIONS: &[OptionSpec] = &[
    value("--disk"),
    value("--target"),
    value("--mode"),
    value("--partition"),
    value("--boot-mib"),
    value("--boot-sectors"),
    value("--boot-start-sector"),
    value("--share-mib"),
    value("--share-sectors"),
    value("--share-start-sector"),
    value("--encrypt-mib"),
    value("--encrypt-sectors"),
    value("--encrypt-start-sector"),
    value("--label-id"),
    value("--user"),
    value("--dept"),
    value("--label"),
    value("--password"),
    value("--volume-label"),
    switch("--format-boot"),
    switch("--format-share"),
    switch("--format-encrypt"),
    value("--boot-label"),
    value("--share-label"),
    value("--encrypt-label"),
    value("--boot-fs"),
    value("--share-fs"),
    value("--encrypt-fs"),
    switch("--force-change-password"),
    switch("--no-force-change-password"),
    switch("--cancel-password-complexity-check"),
    switch("--enforce-password-complexity-check"),
    value("--share-max-password-errors"),
    value("--encrypt-max-password-errors"),
    HELP,
];
const PROVISION_PLAN_OPTIONS: &[OptionSpec] = &[];
const PROVISION_IMAGE_OPTIONS: &[OptionSpec] = &[value("--out")];
const PROVISION_WRITE_OPTIONS: &[OptionSpec] = &[switch("--yes")];

const PROVISION_ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "plan",
        summary: "只读计算制盘计划",
        options: PROVISION_PLAN_OPTIONS,
    },
    ActionSpec {
        name: "image",
        summary: "导出稀疏制盘镜像",
        options: PROVISION_IMAGE_OPTIONS,
    },
    ActionSpec {
        name: "write",
        summary: "写入物理盘",
        options: PROVISION_WRITE_OPTIONS,
    },
];

const INSPECT_ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "raw",
        summary: "查看物理原始字节",
        options: EMPTY_OPTIONS,
    },
    ActionSpec {
        name: "decode",
        summary: "查看解码字节",
        options: EMPTY_OPTIONS,
    },
    ActionSpec {
        name: "meta",
        summary: "查看结构化字段语义",
        options: EMPTY_OPTIONS,
    },
];

const COMPLETION_ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "zsh",
        summary: "生成 zsh 补全脚本",
        options: EMPTY_OPTIONS,
    },
    ActionSpec {
        name: "bash",
        summary: "生成 bash 补全脚本",
        options: EMPTY_OPTIONS,
    },
    ActionSpec {
        name: "fish",
        summary: "生成 fish 补全脚本",
        options: EMPTY_OPTIONS,
    },
];

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "list",
        summary: "查看当前插入的 U 盘",
        usage: "edpcli list [--backup-dir D]",
        options: LIST_OPTIONS,
        actions: &[],
    },
    CommandSpec {
        name: "tui",
        summary: "交互式 TUI（Vim 键位）",
        usage: "edpcli tui",
        options: EMPTY_OPTIONS,
        actions: &[],
    },
    CommandSpec {
        name: "info",
        summary: "查看 U 盘或备份详细信息",
        usage: "edpcli info [备份.edpb] [--disk N] [--id DEVICE_ID] [--backup-dir D]",
        options: INFO_OPTIONS,
        actions: &[],
    },
    CommandSpec {
        name: "backup",
        summary: "创建、查看、校验、恢复和清理备份",
        usage: "edpcli backup [create|list|restore|verify|delete|prune] [选项]",
        options: EMPTY_OPTIONS,
        actions: BACKUP_ACTIONS,
    },
    CommandSpec {
        name: "provision",
        summary: "mode0～mode3 官方模式与 Plain 普通盘制盘",
        usage: "edpcli provision <plan|image|write> [选项]",
        options: PROVISION_COMMON_OPTIONS,
        actions: PROVISION_ACTIONS,
    },
    CommandSpec {
        name: "inspect",
        summary: "高级：检查底层 LBA/hex 数据",
        usage: "edpcli inspect <raw|decode|meta> [备份.edpb] [选项]",
        options: INSPECT_OPTIONS,
        actions: INSPECT_ACTIONS,
    },
    CommandSpec {
        name: "completion",
        summary: "生成 Shell 补全",
        usage: "edpcli completion <zsh|bash|fish>",
        options: EMPTY_OPTIONS,
        actions: COMPLETION_ACTIONS,
    },
    CommandSpec {
        name: "version",
        summary: "版本与构建信息",
        usage: "edpcli version",
        options: EMPTY_OPTIONS,
        actions: &[],
    },
    CommandSpec {
        name: "help",
        summary: "帮助",
        usage: "edpcli help [命令]",
        options: EMPTY_OPTIONS,
        actions: &[],
    },
];

pub fn top_level_specs() -> &'static [CommandSpec] {
    COMMANDS
}

pub fn command(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|command| command.name == name)
}

pub fn words(values: impl IntoIterator<Item = &'static str>) -> String {
    values.into_iter().collect::<Vec<_>>().join(" ")
}
