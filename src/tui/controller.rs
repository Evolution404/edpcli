mod provision;

use super::clipboard::ClipboardBackend;
use super::keymap::{TuiAction, WidgetRole};
use super::pane::PaneId;
use super::state::{
    AdvancedInspectSource, AdvancedInspectStage, AppState, DeviceInfoNodeKey, InspectViewMode,
    NavCommand, StateEffect, Workspace,
};

#[derive(Debug, Clone)]
pub(super) enum ActionRequest {
    Navigate(NavCommand),
    InspectSelection {
        force_hex: bool,
    },
    InspectPreview {
        source: AdvancedInspectSource,
        lba: u64,
    },
    ProvisionKeyProbe {
        disk: u32,
    },
    ProvisionSourcePasswordVerify,
    ProvisionPlan,
}

#[derive(Debug)]
pub(super) struct ActionOutcome {
    pub handled: bool,
    pub effect: StateEffect,
    pub request: Option<ActionRequest>,
}

impl ActionOutcome {
    fn handled() -> Self {
        Self {
            handled: true,
            effect: StateEffect::None,
            request: None,
        }
    }

    fn effect(effect: StateEffect) -> Self {
        Self {
            handled: true,
            effect,
            request: None,
        }
    }

    fn request(request: ActionRequest) -> Self {
        Self {
            handled: true,
            effect: StateEffect::None,
            request: Some(request),
        }
    }

    fn unhandled() -> Self {
        Self {
            handled: false,
            effect: StateEffect::None,
            request: None,
        }
    }
}

fn table_action(action: TuiAction) -> bool {
    matches!(
        action,
        TuiAction::TableColumnLeft
            | TuiAction::TableColumnRight
            | TuiAction::TableMoveColumnLeft
            | TuiAction::TableMoveColumnRight
            | TuiAction::TableColumnFirst
            | TuiAction::TableColumnLast
            | TuiAction::TableScrollLeft
            | TuiAction::TableScrollRight
            | TuiAction::TableSortToggle
            | TuiAction::TableSortClear
            | TuiAction::TableCopyCell
            | TuiAction::TableCopyRow
    )
}

fn action_to_nav(action: TuiAction) -> Option<NavCommand> {
    Some(match action {
        TuiAction::MoveUp => NavCommand::Up,
        TuiAction::MoveDown => NavCommand::Down,
        TuiAction::Top => NavCommand::Top,
        TuiAction::Bottom => NavCommand::Bottom,
        TuiAction::HalfPageUp => NavCommand::HalfPageUp,
        TuiAction::HalfPageDown => NavCommand::HalfPageDown,
        TuiAction::Back => NavCommand::Escape,
        TuiAction::Quit => NavCommand::Quit,
        TuiAction::Help => NavCommand::Help,
        TuiAction::Search => NavCommand::Search,
        TuiAction::NextMatch => NavCommand::NextMatch,
        TuiAction::PreviousMatch => NavCommand::PreviousMatch,
        TuiAction::Command => NavCommand::CommandPalette,
        TuiAction::Refresh => NavCommand::Refresh,
        _ => return None,
    })
}

fn navigate_pure(
    state: &mut AppState,
    command: NavCommand,
    viewport_height: usize,
) -> ActionOutcome {
    if command == NavCommand::Refresh {
        ActionOutcome::request(ActionRequest::Navigate(command))
    } else {
        ActionOutcome::effect(state.navigate(command, viewport_height))
    }
}

fn open_inspect_selection(state: &mut AppState) {
    match state.advanced_inspect_focused_pane() {
        Some(PaneId::InspectDetail) if state.advanced_inspect_detail_selected_row().is_some() => {
            state.advanced_inspect_detail_toggle_selected();
        }
        _ => state.advanced_inspect_toggle_selected(),
    }
}

fn move_inspect(state: &mut AppState, delta: isize, viewport_height: usize) -> ActionOutcome {
    let content_len = state.advanced_inspect_focused_content_len();
    state.advanced_inspect_move_focused_vertical(delta, viewport_height, content_len);
    ActionOutcome::handled()
}

fn dispatch_inspect(
    state: &mut AppState,
    action: TuiAction,
    role: WidgetRole,
    viewport_height: usize,
) -> Option<ActionOutcome> {
    let browser = state
        .advanced_inspect()
        .is_some_and(|advanced| advanced.stage == AdvancedInspectStage::Browser);
    if !browser {
        return None;
    }

    Some(match action {
        TuiAction::InspectJump => {
            state.advanced_inspect_begin_jump();
            ActionOutcome::handled()
        }
        TuiAction::Search => {
            state.advanced_inspect_begin_search();
            ActionOutcome::handled()
        }
        TuiAction::NextMatch | TuiAction::PreviousMatch => {
            if let Err(message) =
                state.advanced_inspect_search_next(action == TuiAction::PreviousMatch)
            {
                state.set_notice(message);
            }
            ActionOutcome::handled()
        }
        TuiAction::MoveUp => move_inspect(state, -1, 1),
        TuiAction::MoveDown => move_inspect(state, 1, 1),
        TuiAction::MoveLeft if role == WidgetRole::Tree => {
            state.advanced_inspect_collapse_or_parent();
            ActionOutcome::handled()
        }
        TuiAction::MoveRight if role == WidgetRole::Tree => {
            state.advanced_inspect_expand_or_child();
            ActionOutcome::handled()
        }
        TuiAction::Top => {
            state.advanced_inspect_focused_top();
            ActionOutcome::handled()
        }
        TuiAction::Bottom => {
            state.advanced_inspect_focused_bottom();
            ActionOutcome::handled()
        }
        TuiAction::Open => {
            open_inspect_selection(state);
            ActionOutcome::handled()
        }
        TuiAction::Yank | TuiAction::YankRaw => {
            if state.advanced_inspect_focused_pane() == Some(PaneId::InspectDetail) {
                let _ = state.advanced_inspect_detail_yank(action == TuiAction::YankRaw);
            }
            ActionOutcome::handled()
        }
        TuiAction::Refresh => {
            if let Some((source, lba)) = state.advanced_inspect_retry_selected_preview() {
                ActionOutcome::request(ActionRequest::InspectPreview { source, lba })
            } else {
                ActionOutcome::handled()
            }
        }
        TuiAction::Activate => {
            ActionOutcome::request(ActionRequest::InspectSelection { force_hex: false })
        }
        TuiAction::InspectBusiness => {
            state.advanced_inspect_set_view_mode(InspectViewMode::Business);
            ActionOutcome::handled()
        }
        TuiAction::InspectRawFields => {
            state.advanced_inspect_set_view_mode(InspectViewMode::RawFields);
            ActionOutcome::handled()
        }
        TuiAction::InspectHex => {
            state.advanced_inspect_set_view_mode(InspectViewMode::Hex);
            if state.advanced_inspect_sector().is_some() {
                return Some(ActionOutcome::handled());
            }
            ActionOutcome::request(ActionRequest::InspectSelection { force_hex: true })
        }
        TuiAction::PanelNext | TuiAction::PanelPrevious => {
            state.advanced_inspect_shift_panel(action == TuiAction::PanelPrevious);
            ActionOutcome::handled()
        }
        TuiAction::PanelLeft => {
            state.advanced_inspect_spatial_focus(-1, 0);
            ActionOutcome::handled()
        }
        TuiAction::PanelRight => {
            state.advanced_inspect_spatial_focus(1, 0);
            ActionOutcome::handled()
        }
        TuiAction::PanelUp => {
            state.advanced_inspect_spatial_focus(0, -1);
            ActionOutcome::handled()
        }
        TuiAction::PanelDown => {
            state.advanced_inspect_spatial_focus(0, 1);
            ActionOutcome::handled()
        }
        TuiAction::HalfPageUp => move_inspect(state, -((viewport_height / 2).max(1) as isize), 1),
        TuiAction::HalfPageDown => move_inspect(state, (viewport_height / 2).max(1) as isize, 1),
        TuiAction::PageUp => move_inspect(state, -(viewport_height.max(1) as isize), 1),
        TuiAction::PageDown => move_inspect(state, viewport_height.max(1) as isize, 1),
        TuiAction::Back => ActionOutcome::effect(state.navigate(NavCommand::Escape, 1)),
        TuiAction::Help => ActionOutcome::effect(state.navigate(NavCommand::Help, 1)),
        _ => return None,
    })
}

pub(super) fn active_widget_role(state: &AppState) -> WidgetRole {
    if state.workspace() == Workspace::Inspect {
        if state
            .advanced_inspect()
            .is_some_and(|advanced| advanced.panel == super::state::AdvancedInspectPanel::Tree)
        {
            return WidgetRole::Tree;
        }
        if state.advanced_inspect().is_some_and(|advanced| {
            advanced.panel == super::state::AdvancedInspectPanel::Detail
                && state
                    .advanced_inspect_selected_sector_lba()
                    .is_some_and(|lba| {
                        advanced.result.as_ref().is_some_and(|workspace| {
                            workspace
                                .items
                                .iter()
                                .any(|item| item.lba == lba && !item.fields.is_empty())
                        })
                    })
        }) {
            return WidgetRole::Table;
        }
        return WidgetRole::Other;
    }

    if state.provision_scheme_picker_open() {
        return WidgetRole::Picker;
    }

    if state.active_table_kind().is_some() {
        WidgetRole::Table
    } else {
        WidgetRole::Other
    }
}

/// Interpret one logical TUI action. This is the single state-transition router shared by
/// the production loop and demo loop. Requests that require external work are returned to the
/// caller so production can use TaskHub while demo remains unable to reach real media I/O.
pub(super) fn dispatch_action(
    state: &mut AppState,
    action: TuiAction,
    role: WidgetRole,
    viewport_height: usize,
    viewport_width: u16,
    clipboard: &mut dyn ClipboardBackend,
) -> ActionOutcome {
    if state.help_open() {
        return match action {
            TuiAction::Back => ActionOutcome::effect(state.navigate(NavCommand::Escape, 1)),
            TuiAction::Help => ActionOutcome::effect(state.navigate(NavCommand::Help, 1)),
            TuiAction::Quit => {
                ActionOutcome::effect(state.navigate(NavCommand::Quit, viewport_height))
            }
            _ => ActionOutcome::handled(),
        };
    }

    if action == TuiAction::Help {
        return ActionOutcome::effect(state.navigate(NavCommand::Help, viewport_height));
    }

    if state.provision_scheme_picker_open() {
        if let Some(outcome) = provision::dispatch_provision(state, action, viewport_height) {
            return outcome;
        }
    }

    if matches!(action, TuiAction::FocusNext | TuiAction::FocusPrevious) {
        let reverse = action == TuiAction::FocusPrevious;
        return match state.workspace() {
            Workspace::Devices | Workspace::Backups => {
                let command = if reverse {
                    NavCommand::PreviousWorkspace
                } else {
                    NavCommand::NextWorkspace
                };
                ActionOutcome::effect(state.navigate(command, viewport_height))
            }
            Workspace::Inspect => {
                state.advanced_inspect_shift_panel(reverse);
                ActionOutcome::handled()
            }
            Workspace::Provision => {
                state.provision_tab_focus(reverse);
                ActionOutcome::handled()
            }
        };
    }

    if action == TuiAction::Quit {
        return ActionOutcome::effect(state.navigate(NavCommand::Quit, viewport_height));
    }

    if table_action(action) && role == WidgetRole::Table {
        let _ = super::table_dispatch::dispatch_table_action_with_clipboard(
            state,
            action,
            viewport_height,
            viewport_width,
            clipboard,
        );
        return ActionOutcome::handled();
    }

    if state.workspace() == Workspace::Inspect {
        if let Some(outcome) = dispatch_inspect(state, action, role, viewport_height) {
            return outcome;
        }
    }

    if state.workspace() == Workspace::Provision {
        if let Some(outcome) = provision::dispatch_provision(state, action, viewport_height) {
            return outcome;
        }
    }

    match action {
        TuiAction::Insert
            if matches!(state.workspace(), Workspace::Devices | Workspace::Backups) =>
        {
            ActionOutcome::request(ActionRequest::Navigate(NavCommand::OpenInspect))
        }
        TuiAction::Activate => match state.workspace() {
            Workspace::Devices => {
                if state.devices_focused_pane() == PaneId::DevicesTree {
                    state.device_info_focus_detail();
                } else if state.devices_focused_pane() == PaneId::DevicesList {
                    if let Err(message) = state.activate_device_for_viewport(viewport_width) {
                        state.set_notice(message);
                    }
                }
                ActionOutcome::handled()
            }
            Workspace::Backups => {
                ActionOutcome::request(ActionRequest::Navigate(NavCommand::OpenInspect))
            }
            Workspace::Inspect | Workspace::Provision => ActionOutcome::unhandled(),
        },
        TuiAction::Provision if state.workspace() == Workspace::Devices => {
            if state.devices_focused_pane() != PaneId::DevicesList {
                state.set_notice("请先回到设备列表，再按 p 选择制盘方案。");
            } else if let Err(message) = state.begin_provision_for_selected_device() {
                state.set_notice(message);
            }
            ActionOutcome::handled()
        }
        TuiAction::Open if state.workspace() == Workspace::Devices => {
            if state.devices_focused_pane() == PaneId::DevicesTree {
                state.device_info_toggle_selected();
            }
            ActionOutcome::handled()
        }
        TuiAction::Toggle if state.workspace() == Workspace::Backups => {
            ActionOutcome::request(ActionRequest::Navigate(NavCommand::ToggleBackupSelection))
        }
        TuiAction::ViewOrVerify if state.workspace() == Workspace::Backups => {
            ActionOutcome::request(ActionRequest::Navigate(NavCommand::VerifyBackup))
        }
        TuiAction::Delete if state.workspace() == Workspace::Backups => {
            let command = if state.backup_selection_count() > 0 {
                NavCommand::BeginBackupBatchDelete
            } else {
                NavCommand::BeginBackupDelete
            };
            ActionOutcome::request(ActionRequest::Navigate(command))
        }
        TuiAction::BackupCreate
            if matches!(state.workspace(), Workspace::Devices | Workspace::Backups) =>
        {
            ActionOutcome::request(ActionRequest::Navigate(NavCommand::BeginBackupCreate))
        }
        TuiAction::Restore
            if state.workspace() == Workspace::Backups
                || (state.workspace() == Workspace::Devices
                    && state.devices_focused_pane() == PaneId::DevicesDetail
                    && matches!(
                        state.device_info_selected_key(),
                        DeviceInfoNodeKey::Status | DeviceInfoNodeKey::Backups
                    )
                    && state.selected_device_related_backup().is_some()) =>
        {
            ActionOutcome::request(ActionRequest::Navigate(NavCommand::BeginRestore))
        }
        TuiAction::PanelNext | TuiAction::PanelPrevious
            if matches!(state.workspace(), Workspace::Devices | Workspace::Backups) =>
        {
            state.shift_workspace_pane(action == TuiAction::PanelPrevious);
            ActionOutcome::handled()
        }
        TuiAction::PanelLeft
        | TuiAction::PanelRight
        | TuiAction::PanelUp
        | TuiAction::PanelDown
            if matches!(state.workspace(), Workspace::Devices | Workspace::Backups) =>
        {
            let (dx, dy) = match action {
                TuiAction::PanelLeft => (-1, 0),
                TuiAction::PanelRight => (1, 0),
                TuiAction::PanelUp => (0, -1),
                TuiAction::PanelDown => (0, 1),
                _ => unreachable!(),
            };
            state.spatial_workspace_focus(dx, dy);
            ActionOutcome::handled()
        }
        _ => {
            if let Some(command) = action_to_nav(action) {
                navigate_pure(state, command, viewport_height)
            } else {
                ActionOutcome::unhandled()
            }
        }
    }
}
