//! Contextual action descriptions shared by help, feedback and availability checks.
use super::{
    keymap::{HelpBinding, TuiAction},
    pane::PaneId,
    state::{AppState, InputMode, Workspace},
};
#[derive(Debug, Clone, Copy)]
pub(super) enum ActionRisk {
    ReadOnly,
    Write,
    Delete,
}
#[derive(Debug)]
pub(super) struct ActionScope {
    pub workspace: Workspace,
    pub pane: Option<PaneId>,
    pub input: InputMode,
    pub modal: bool,
}
#[derive(Debug)]
pub(super) struct ActionSpec {
    pub action: TuiAction,
    pub keys: &'static str,
    pub label: &'static str,
    pub scope: ActionScope,
    pub risk: ActionRisk,
    pub disabled_reason: Option<String>,
}

impl ActionSpec {
    fn applies_to(&self, state: &AppState) -> bool {
        let pane = match state.workspace() {
            Workspace::Backups => Some(state.backups_focused_pane()),
            Workspace::Devices => Some(state.devices_focused_pane()),
            Workspace::Provision => Some(state.provision_focused_pane()),
            Workspace::Inspect => None,
        };
        self.scope.workspace == state.workspace()
            && self.scope.pane == pane
            && self.scope.input == state.input_mode()
            && self.scope.modal == (state.help_open() || state.wizard().is_some())
    }
    pub(super) fn available_in(&self, state: &AppState) -> bool {
        self.applies_to(state) && self.disabled_reason.is_none()
    }
}

pub(super) fn unavailable_reason(state: &AppState, action: TuiAction) -> Option<String> {
    if action != TuiAction::Restore {
        return None;
    }
    let row = match state.selected_device() {
        Some(row) => row,
        None => return Some("请先选择目标 USB 设备。".into()),
    };
    let backup = if state.workspace() == Workspace::Backups {
        state.selected_backup()
    } else {
        state.selected_device_related_backup()
    };
    let Some(backup) = backup else {
        return Some("请先选择要恢复的备份。".into());
    };
    if !backup.is_restorable() {
        return Some(format!(
            "备份不可恢复：{}",
            backup
                .verification_error
                .as_deref()
                .unwrap_or("完整性或范围校验未通过")
        ));
    }
    if row.identity_pin.is_none() {
        return Some("目标介质身份尚未完成只读采集，请刷新设备后重试。".into());
    }
    if state.selected_restore_backup_path().is_none() {
        return Some("请在设备详情的备份区域选择恢复条目。".into());
    }
    None
}

pub(super) fn describe(state: &AppState, binding: &HelpBinding) -> ActionSpec {
    let pane = match state.workspace() {
        Workspace::Backups => Some(state.backups_focused_pane()),
        Workspace::Devices => Some(state.devices_focused_pane()),
        Workspace::Provision => Some(state.provision_focused_pane()),
        Workspace::Inspect => None,
    };
    ActionSpec {
        action: binding.action,
        keys: binding.keys,
        label: binding.label,
        scope: ActionScope {
            workspace: state.workspace(),
            pane,
            input: state.input_mode(),
            modal: state.help_open() || state.wizard().is_some(),
        },
        risk: match binding.action {
            TuiAction::Restore | TuiAction::Provision => ActionRisk::Write,
            TuiAction::Delete => ActionRisk::Delete,
            _ => ActionRisk::ReadOnly,
        },
        disabled_reason: unavailable_reason(state, binding.action),
    }
}

pub(super) fn context_hint(state: &AppState) -> String {
    let context = super::help_context::resolve(state);
    let wanted: &[TuiAction] = match state.workspace() {
        Workspace::Provision => &[TuiAction::Insert, TuiAction::Activate, TuiAction::PanelNext],
        Workspace::Backups if state.viewport_size().width < 80 => {
            &[TuiAction::PanelNext, TuiAction::Activate]
        }
        Workspace::Backups => &[
            TuiAction::Activate,
            TuiAction::ViewOrVerify,
            TuiAction::Restore,
            TuiAction::Delete,
            TuiAction::PanelNext,
        ],
        _ => &[
            TuiAction::Activate,
            TuiAction::Provision,
            TuiAction::BackupCreate,
        ],
    };
    let mut hints = Vec::new();
    for action in wanted {
        if let Some(binding) = context
            .bindings
            .iter()
            .find(|binding| binding.action == *action)
        {
            let spec = describe(state, binding);
            // Context and risk are explicit facts, so a future menu cannot treat this as a global binding.
            debug_assert!(spec.applies_to(state));
            let key = spec.keys.split('·').next().unwrap_or(spec.keys).trim();
            let key = if spec.action == TuiAction::PanelNext {
                "Ctrl-w w"
            } else {
                key
            };
            let label = if state.viewport_size().width < 80 {
                match spec.action {
                    TuiAction::PanelNext => "切窗",
                    TuiAction::Activate => "打开",
                    TuiAction::Insert => "编辑",
                    _ => spec.label,
                }
            } else {
                spec.label.split(' ').next().unwrap_or(spec.label)
            };
            hints.push(format!("{key} {label}"));
        }
    }
    hints.push("? 帮助".into());
    hints.join(" · ")
}
