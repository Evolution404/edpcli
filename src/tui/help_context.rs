use super::{
    keymap::{
        HelpBinding, BACKUPS_HELP, BUSY_GLOBAL_HELP, DEVICES_HELP, GLOBAL_HELP, GUARD_GLOBAL_HELP,
        INSPECT_HELP, PICKER_HELP, PROVISION_HELP, PROVISION_RESULT_HELP, PROVISION_REVIEW_HELP,
        PROVISION_RUNNING_HELP, RESTORE_RESULT_HELP, SECTOR_INSPECT_HELP,
    },
    state::{
        AppState, BackupBatchDeleteStage, BackupPruneStage, InspectViewMode, ProvisionStage,
        WizardStage, Workspace,
    },
};

pub(super) struct HelpContext {
    pub title: &'static str,
    pub bindings: &'static [HelpBinding],
    pub global_title: &'static str,
    pub global: &'static [HelpBinding],
    pub include_table: bool,
}

pub(super) fn resolve(state: &AppState) -> HelpContext {
    let sector_hex = state.workspace() == Workspace::Inspect
        && state.advanced_inspect().is_some_and(|advanced| {
            advanced.view_mode == InspectViewMode::Hex && advanced.sector.is_some()
        });
    let busy = matches!(
        state.provision().stage,
        ProvisionStage::Planning | ProvisionStage::Exporting
    ) || state
        .backup_batch_delete()
        .is_some_and(|batch| batch.stage == BackupBatchDeleteStage::Planning)
        || state
            .backup_prune()
            .is_some_and(|prune| prune.stage == BackupPruneStage::Planning);
    let critical = state.is_critical_operation();

    let (title, bindings) = if state.workspace() == Workspace::Provision
        && state.provision().stage == ProvisionStage::Planning
    {
        ("制盘计划生成中", &[][..])
    } else if state.workspace() == Workspace::Provision
        && state.provision().stage == ProvisionStage::Exporting
    {
        ("镜像导出中", &[][..])
    } else if state
        .backup_batch_delete()
        .is_some_and(|batch| batch.stage == BackupBatchDeleteStage::Planning)
    {
        ("批量删除计划生成中", &[][..])
    } else if state
        .backup_prune()
        .is_some_and(|prune| prune.stage == BackupPruneStage::Planning)
    {
        ("备份清理计划生成中", &[][..])
    } else if state.workspace() == Workspace::Provision
        && state.provision().stage == ProvisionStage::Running
    {
        ("制盘执行", PROVISION_RUNNING_HELP)
    } else if critical {
        ("关键操作执行中", &[][..])
    } else if state.provision_scheme_picker_open() {
        ("选择制盘方案", PICKER_HELP)
    } else if state
        .wizard()
        .is_some_and(|wizard| wizard.stage == WizardStage::PostRestore)
    {
        ("恢复结果", RESTORE_RESULT_HELP)
    } else if state.workspace() == Workspace::Provision
        && state.provision().stage == ProvisionStage::Review
    {
        ("计划确认", PROVISION_REVIEW_HELP)
    } else if state.workspace() == Workspace::Provision
        && state.provision().stage == ProvisionStage::Result
    {
        ("制盘结果", PROVISION_RESULT_HELP)
    } else if sector_hex {
        ("Hex 详情", SECTOR_INSPECT_HELP)
    } else {
        match state.workspace() {
            Workspace::Devices => ("设备", DEVICES_HELP),
            Workspace::Backups => ("备份", BACKUPS_HELP),
            Workspace::Inspect => ("检查", INSPECT_HELP),
            Workspace::Provision => ("制盘", PROVISION_HELP),
        }
    };

    let (global_title, global) = if critical {
        ("安全操作", GUARD_GLOBAL_HELP)
    } else if busy {
        ("后台处理中", BUSY_GLOBAL_HELP)
    } else {
        ("全局", GLOBAL_HELP)
    };

    HelpContext {
        title,
        bindings,
        global_title,
        global,
        include_table: !critical
            && !busy
            && !sector_hex
            && !state.provision_scheme_picker_open()
            && state.active_table_kind().is_some(),
    }
}
