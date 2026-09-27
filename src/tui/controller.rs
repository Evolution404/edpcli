use super::clipboard::ClipboardBackend;
use super::keymap::{TuiAction, WidgetRole};
use super::pane::PaneId;
use super::state::{
    AdvancedInspectSource, AdvancedInspectStage, AppState, NavCommand, ProvisionKind,
    ProvisionStage, StateEffect, Workspace,
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
        TuiAction::WorkspaceNext => NavCommand::NextWorkspace,
        TuiAction::WorkspacePrevious => NavCommand::PreviousWorkspace,
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

fn show_inspect_layout_detail(state: &mut AppState) {
    let detail = state
        .advanced_inspect()
        .and_then(|advanced| advanced.result.as_ref())
        .and_then(|workspace| workspace.disk_layout.as_ref())
        .and_then(|model| state.disk_layout_detail(model));
    if let Some(detail) = detail {
        state.set_notice(detail);
    }
}

fn open_inspect_selection(state: &mut AppState) {
    match state.advanced_inspect_focused_pane() {
        Some(PaneId::InspectDiskLayout) => state.toggle_disk_layout_tail(),
        Some(PaneId::InspectDetail) if state.advanced_inspect_detail_selected_row().is_some() => {
            state.advanced_inspect_detail_toggle_selected();
        }
        _ => state.advanced_inspect_toggle_selected(),
    }
}

fn show_provision_layout_detail(state: &mut AppState) {
    let model = state.provision_layout_model();
    if let Some(detail) = state.disk_layout_detail(&model) {
        state.set_notice(detail);
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
            if state.advanced_inspect_focused_pane() == Some(PaneId::InspectDiskLayout) {
                show_inspect_layout_detail(state);
                ActionOutcome::handled()
            } else {
                ActionOutcome::request(ActionRequest::InspectSelection { force_hex: false })
            }
        }
        TuiAction::InspectBusiness => {
            state.advanced_inspect_focus_pane(PaneId::InspectOverview);
            state.pane_viewport_mut(PaneId::InspectDetail).scroll_x = 0;
            ActionOutcome::handled()
        }
        TuiAction::InspectRawFields => {
            state.advanced_inspect_focus_pane(PaneId::InspectDetail);
            state.pane_viewport_mut(PaneId::InspectDetail).scroll_x = 2;
            ActionOutcome::handled()
        }
        TuiAction::InspectHex => {
            ActionOutcome::request(ActionRequest::InspectSelection { force_hex: true })
        }
        TuiAction::InspectDiskLayout => {
            state.advanced_inspect_focus_pane(PaneId::InspectDiskLayout);
            ActionOutcome::handled()
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

fn move_provision(state: &mut AppState, delta: isize, viewport_height: usize) -> ActionOutcome {
    let content_len = state.provision_focused_content_len();
    state.provision_move_focused_vertical(delta, viewport_height, content_len);
    ActionOutcome::handled()
}

fn dispatch_provision(
    state: &mut AppState,
    action: TuiAction,
    viewport_height: usize,
) -> Option<ActionOutcome> {
    if state.workspace() != Workspace::Provision {
        return None;
    }

    let stage = state.provision().stage;
    Some(match stage {
        ProvisionStage::SelectDisk => match action {
            TuiAction::MoveUp => {
                ActionOutcome::effect(state.navigate(NavCommand::Up, viewport_height))
            }
            TuiAction::MoveDown => {
                ActionOutcome::effect(state.navigate(NavCommand::Down, viewport_height))
            }
            TuiAction::Top => {
                ActionOutcome::effect(state.navigate(NavCommand::Top, viewport_height))
            }
            TuiAction::Bottom => {
                ActionOutcome::effect(state.navigate(NavCommand::Bottom, viewport_height))
            }
            TuiAction::Activate => {
                if state.provision_select_disk().is_none() {
                    state.set_notice("请选择可读取的 USB 整盘目标。");
                }
                ActionOutcome::handled()
            }
            TuiAction::Back => {
                ActionOutcome::effect(state.navigate(NavCommand::Escape, viewport_height))
            }
            _ => return None,
        },
        ProvisionStage::Menu => match action {
            TuiAction::MoveUp => {
                ActionOutcome::effect(state.navigate(NavCommand::Up, viewport_height))
            }
            TuiAction::MoveDown => {
                ActionOutcome::effect(state.navigate(NavCommand::Down, viewport_height))
            }
            TuiAction::Top => {
                ActionOutcome::effect(state.navigate(NavCommand::Top, viewport_height))
            }
            TuiAction::Bottom => {
                ActionOutcome::effect(state.navigate(NavCommand::Bottom, viewport_height))
            }
            TuiAction::Activate => {
                let Some(disk) = state.selected_device_disk() else {
                    state.set_notice("物理制盘需要先在制盘页明确选择 USB 目标。");
                    return Some(ActionOutcome::handled());
                };
                let kind = state.provision_begin_selected();
                if kind == ProvisionKind::Plain {
                    ActionOutcome::handled()
                } else {
                    ActionOutcome::request(ActionRequest::ProvisionKeyProbe { disk })
                }
            }
            TuiAction::Back => {
                ActionOutcome::effect(state.navigate(NavCommand::Escape, viewport_height))
            }
            _ => return None,
        },
        ProvisionStage::Form if state.input_mode() == super::state::InputMode::Normal => {
            match action {
                TuiAction::Open
                    if state.provision_focused_pane() == PaneId::ProvisionDiskLayout =>
                {
                    state.toggle_disk_layout_tail();
                    ActionOutcome::handled()
                }
                TuiAction::PanelNext | TuiAction::PanelPrevious => {
                    state.provision_shift_pane(action == TuiAction::PanelPrevious);
                    ActionOutcome::handled()
                }
                TuiAction::PanelLeft => {
                    state.provision_spatial_focus(-1, 0);
                    ActionOutcome::handled()
                }
                TuiAction::PanelRight => {
                    state.provision_spatial_focus(1, 0);
                    ActionOutcome::handled()
                }
                TuiAction::PanelUp => {
                    state.provision_spatial_focus(0, -1);
                    ActionOutcome::handled()
                }
                TuiAction::PanelDown => {
                    state.provision_spatial_focus(0, 1);
                    ActionOutcome::handled()
                }
                TuiAction::MoveUp => move_provision(state, -1, viewport_height),
                TuiAction::MoveDown => move_provision(state, 1, viewport_height),
                TuiAction::Top => {
                    state.provision_focused_top();
                    ActionOutcome::handled()
                }
                TuiAction::Bottom => {
                    state.provision_focused_bottom(viewport_height);
                    ActionOutcome::handled()
                }
                TuiAction::HalfPageUp => move_provision(
                    state,
                    -((viewport_height / 2).max(1) as isize),
                    viewport_height,
                ),
                TuiAction::HalfPageDown => move_provision(
                    state,
                    (viewport_height / 2).max(1) as isize,
                    viewport_height,
                ),
                TuiAction::PageUp => {
                    move_provision(state, -(viewport_height.max(1) as isize), viewport_height)
                }
                TuiAction::PageDown => {
                    move_provision(state, viewport_height.max(1) as isize, viewport_height)
                }
                TuiAction::MoveLeft | TuiAction::MoveRight | TuiAction::Toggle
                    if state.provision_focused_pane() == PaneId::ProvisionParameters =>
                {
                    state.provision_toggle_selected_option();
                    ActionOutcome::handled()
                }
                TuiAction::Insert
                    if state.provision_focused_pane() == PaneId::ProvisionParameters =>
                {
                    state.provision_begin_insert();
                    ActionOutcome::handled()
                }
                TuiAction::Fill
                    if state.provision_focused_pane() == PaneId::ProvisionParameters =>
                {
                    state.provision_fill_selected_capacity();
                    ActionOutcome::handled()
                }
                TuiAction::Add if state.provision_focused_pane() == PaneId::ProvisionParameters => {
                    state.provision_plain_add_partition();
                    ActionOutcome::handled()
                }
                TuiAction::Delete
                    if state.provision_focused_pane() == PaneId::ProvisionParameters =>
                {
                    state.provision_plain_delete_selected_partition();
                    ActionOutcome::handled()
                }
                TuiAction::ViewOrVerify
                    if state.provision_focused_pane() == PaneId::ProvisionParameters =>
                {
                    ActionOutcome::request(ActionRequest::ProvisionSourcePasswordVerify)
                }
                TuiAction::Activate => {
                    if state.provision_focused_pane() == PaneId::ProvisionDiskLayout {
                        show_provision_layout_detail(state);
                        ActionOutcome::handled()
                    } else {
                        ActionOutcome::request(ActionRequest::ProvisionPlan)
                    }
                }
                TuiAction::Export => {
                    state.set_notice("请先按 Enter 生成只读计划，再从计划页导出镜像。");
                    ActionOutcome::handled()
                }
                TuiAction::Back => {
                    ActionOutcome::effect(state.navigate(NavCommand::Escape, viewport_height))
                }
                _ => return None,
            }
        }
        ProvisionStage::Review => match action {
            TuiAction::Open if state.provision_focused_pane() == PaneId::ProvisionDiskLayout => {
                state.toggle_disk_layout_tail();
                ActionOutcome::handled()
            }
            TuiAction::PanelNext | TuiAction::PanelPrevious => {
                state.provision_shift_pane(action == TuiAction::PanelPrevious);
                ActionOutcome::handled()
            }
            TuiAction::PanelLeft => {
                state.provision_spatial_focus(-1, 0);
                ActionOutcome::handled()
            }
            TuiAction::PanelRight => {
                state.provision_spatial_focus(1, 0);
                ActionOutcome::handled()
            }
            TuiAction::PanelUp => {
                state.provision_spatial_focus(0, -1);
                ActionOutcome::handled()
            }
            TuiAction::PanelDown => {
                state.provision_spatial_focus(0, 1);
                ActionOutcome::handled()
            }
            TuiAction::MoveUp => move_provision(state, -1, viewport_height),
            TuiAction::MoveDown => move_provision(state, 1, viewport_height),
            TuiAction::Top => {
                state.provision_focused_top();
                ActionOutcome::handled()
            }
            TuiAction::Bottom => {
                state.provision_focused_bottom(viewport_height);
                ActionOutcome::handled()
            }
            TuiAction::HalfPageUp => move_provision(
                state,
                -((viewport_height / 2).max(1) as isize),
                viewport_height,
            ),
            TuiAction::HalfPageDown => move_provision(
                state,
                (viewport_height / 2).max(1) as isize,
                viewport_height,
            ),
            TuiAction::PageUp => {
                move_provision(state, -(viewport_height.max(1) as isize), viewport_height)
            }
            TuiAction::PageDown => {
                move_provision(state, viewport_height.max(1) as isize, viewport_height)
            }
            TuiAction::Activate => {
                if state.provision_focused_pane() == PaneId::ProvisionDiskLayout {
                    show_provision_layout_detail(state);
                } else {
                    state.provision_begin_confirm();
                }
                ActionOutcome::handled()
            }
            TuiAction::Export => {
                state.provision_begin_export();
                ActionOutcome::handled()
            }
            TuiAction::Back => {
                ActionOutcome::effect(state.navigate(NavCommand::Escape, viewport_height))
            }
            _ => return None,
        },
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

    if state.workspace() == Workspace::Provision
        && matches!(
            state.provision().stage,
            ProvisionStage::SelectDisk | ProvisionStage::Menu
        )
    {
        return WidgetRole::Table;
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
    if matches!(
        action,
        TuiAction::WorkspaceNext | TuiAction::WorkspacePrevious
    ) {
        let command = if action == TuiAction::WorkspaceNext {
            NavCommand::NextWorkspace
        } else {
            NavCommand::PreviousWorkspace
        };
        return ActionOutcome::effect(state.navigate(command, viewport_height));
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
        if let Some(outcome) = dispatch_provision(state, action, viewport_height) {
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
                if let Err(message) = state.activate_device_for_viewport(viewport_width) {
                    state.set_notice(message);
                }
                ActionOutcome::handled()
            }
            Workspace::Backups => {
                ActionOutcome::request(ActionRequest::Navigate(NavCommand::OpenInspect))
            }
            Workspace::Inspect | Workspace::Provision => ActionOutcome::unhandled(),
        },
        TuiAction::Provision if state.workspace() == Workspace::Devices => {
            if let Err(message) = state.begin_provision_for_selected_device() {
                state.set_notice(message);
            }
            ActionOutcome::handled()
        }
        TuiAction::Open if state.workspace() == Workspace::Devices => {
            if state.devices_focused_pane() == PaneId::DevicesSummary {
                state.device_summary_toggle_selected_section();
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
            state.begin_backup_create_choice();
            ActionOutcome::handled()
        }
        TuiAction::Restore if state.workspace() == Workspace::Backups => {
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
