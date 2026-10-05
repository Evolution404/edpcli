use super::state::{
    AdvancedInspectStage, AppState, BackupBatchDeleteStage, BackupPruneStage, InputMode,
    ProvisionStage, WizardStage,
};

pub(super) fn dynamic_status(state: &AppState) -> Option<String> {
    let operation_progress_running = state.provision().stage == ProvisionStage::Running
        || state.wizard().is_some_and(|wizard| {
            matches!(wizard.stage, WizardStage::Running | WizardStage::Formatting)
        });
    if operation_progress_running {
        return None;
    }

    if state.is_critical_operation() && state.backup_delete().is_some() {
        return Some("备份删除正在执行 · 退出请求将在安全检查点处理".into());
    }
    if state.is_critical_operation() && state.backup_batch_delete().is_some() {
        return Some("批量备份删除正在执行 · 退出请求将在安全检查点处理".into());
    }
    if state.is_critical_operation() && state.backup_prune().is_some() {
        return Some("备份清理正在执行 · 退出请求将在安全检查点处理".into());
    }
    if state.is_critical_operation() {
        return Some("关键写盘阶段 · 退出请求将在安全检查点处理".into());
    }

    match state.input_mode() {
        InputMode::Search => {
            return Some(format!("/{} · 搜索输入", state.input_buffer()));
        }
        InputMode::Command => {
            return Some(format!(":{} · 命令输入", state.input_buffer()));
        }
        InputMode::Insert => return Some("INSERT · 正在编辑".into()),
        InputMode::Confirm => return Some("CONFIRM · 等待确认输入".into()),
        InputMode::Normal => {}
    }

    if let Some(advanced) = state.advanced_inspect() {
        if advanced.stage == AdvancedInspectStage::Running {
            return Some("全盘检查 · 后台只读建立结构树…".into());
        }
        if let Some((query, index, total)) = state.advanced_inspect_search_status() {
            return Some(format!("检查搜索 · {index}/{total} · {query}"));
        }
    }

    if let Some(wizard) = state.wizard() {
        return match wizard.stage {
            WizardStage::Running => None,
            WizardStage::Formatting => Some("正在格式化文件系统…".into()),
            WizardStage::Reinitializing => Some("正在重建加密密钥域…".into()),
            _ => None,
        };
    }
    if let Some(batch) = state.backup_batch_delete() {
        return match batch.stage {
            BackupBatchDeleteStage::Planning => Some("正在生成批量删除计划…".into()),
            BackupBatchDeleteStage::Running => Some("正在批量删除备份…".into()),
            _ => None,
        };
    }
    if let Some(prune) = state.backup_prune() {
        return match prune.stage {
            BackupPruneStage::Planning => Some("正在生成备份清理计划…".into()),
            BackupPruneStage::Running => Some("正在清理旧备份…".into()),
            _ => None,
        };
    }

    if state.workspace() == super::state::Workspace::Backups {
        return Some(match state.backups_focused_pane() {
            super::pane::PaneId::BackupCoverage => {
                "j/k 选择容量区域 · Ctrl-w w 切换窗口 · Esc 返回列表".into()
            }
            super::pane::PaneId::BackupSummary => {
                "j/k 滚动元数据 · gg/G 首尾 · Ctrl-w w 切换窗口 · Esc 返回列表".into()
            }
            _ => super::actions::context_hint(state),
        });
    }

    if state.workspace() == super::state::Workspace::Provision
        && state.provision().stage == ProvisionStage::Form
    {
        return Some(super::actions::context_hint(state));
    }

    match state.provision().stage {
        ProvisionStage::Planning => Some("正在生成只读制盘计划…".into()),
        ProvisionStage::Exporting => Some("正在后台导出制盘镜像…".into()),
        ProvisionStage::Running => None,
        _ => None,
    }
}
