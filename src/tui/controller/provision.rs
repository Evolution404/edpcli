use super::{ActionOutcome, ActionRequest};
use crate::tui::keymap::TuiAction;
use crate::tui::pane::PaneId;
use crate::tui::state::{
    AppState, InputMode, NavCommand, ProvisionKind, ProvisionStage, Workspace,
};

fn show_provision_layout_detail(state: &mut AppState) {
    let model = state.provision_layout_model();
    if let Some(detail) = state.disk_layout_detail(&model) {
        state.set_notice(detail);
    }
}

fn move_provision(state: &mut AppState, delta: isize, viewport_height: usize) -> ActionOutcome {
    let content_len = state.provision_focused_content_len();
    state.provision_move_focused_vertical(delta, viewport_height, content_len);
    ActionOutcome::handled()
}

fn activate_scheme(state: &mut AppState) -> ActionOutcome {
    let Some(disk) = state.selected_device_disk() else {
        state.set_notice("物理制盘需要先在设备列表明确选择 USB 目标。");
        return ActionOutcome::handled();
    };
    let kind = state.provision_begin_selected();
    state.provision_enter_form_workspace();
    if kind == ProvisionKind::Plain {
        ActionOutcome::handled()
    } else {
        ActionOutcome::request(ActionRequest::ProvisionKeyProbe { disk })
    }
}

pub(super) fn dispatch_provision(
    state: &mut AppState,
    action: TuiAction,
    viewport_height: usize,
) -> Option<ActionOutcome> {
    if state.provision_scheme_picker_open() {
        return Some(match action {
            TuiAction::MoveUp => {
                state.provision_move_scheme_picker(-1);
                ActionOutcome::handled()
            }
            TuiAction::MoveDown => {
                state.provision_move_scheme_picker(1);
                ActionOutcome::handled()
            }
            TuiAction::Top => {
                state.provision_select_scheme_index(0);
                ActionOutcome::handled()
            }
            TuiAction::Bottom => {
                state.provision_select_scheme_index(ProvisionKind::ALL.len().saturating_sub(1));
                ActionOutcome::handled()
            }
            TuiAction::Activate => activate_scheme(state),
            TuiAction::Back => {
                state.provision_close_scheme_picker();
                ActionOutcome::handled()
            }
            TuiAction::Quit => {
                ActionOutcome::effect(state.navigate(NavCommand::Quit, viewport_height))
            }
            _ => ActionOutcome::handled(),
        });
    }

    if state.workspace() != Workspace::Provision {
        return None;
    }

    let stage = state.provision().stage;
    Some(match stage {
        ProvisionStage::Form if state.input_mode() == InputMode::Normal => match action {
            TuiAction::Open if state.provision_focused_pane() == PaneId::ProvisionParameters => {
                state.provision_toggle_advanced_identity();
                ActionOutcome::handled()
            }
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
            TuiAction::MoveLeft
                if state.provision_focused_pane() == PaneId::ProvisionParameters =>
            {
                state.provision_shift_selected_option(true);
                ActionOutcome::handled()
            }
            TuiAction::MoveRight
                if state.provision_focused_pane() == PaneId::ProvisionParameters =>
            {
                state.provision_shift_selected_option(false);
                ActionOutcome::handled()
            }
            TuiAction::Toggle if state.provision_focused_pane() == PaneId::ProvisionParameters => {
                state.provision_toggle_selected_option();
                ActionOutcome::handled()
            }
            TuiAction::Insert if state.provision_focused_pane() == PaneId::ProvisionParameters => {
                state.provision_begin_insert();
                ActionOutcome::handled()
            }
            TuiAction::Fill if state.provision_focused_pane() == PaneId::ProvisionParameters => {
                state.provision_fill_selected_capacity();
                ActionOutcome::handled()
            }
            TuiAction::Add if state.provision_focused_pane() == PaneId::ProvisionParameters => {
                state.provision_plain_add_partition();
                ActionOutcome::handled()
            }
            TuiAction::Delete if state.provision_focused_pane() == PaneId::ProvisionParameters => {
                state.provision_plain_delete_selected_partition();
                ActionOutcome::handled()
            }
            TuiAction::Activate => ActionOutcome::request(ActionRequest::ProvisionPlan),
            TuiAction::Export => {
                state.set_notice("请先按 Enter 生成只读计划，再从计划页导出镜像。");
                ActionOutcome::handled()
            }
            TuiAction::Back => {
                ActionOutcome::effect(state.navigate(NavCommand::Escape, viewport_height))
            }
            _ => return None,
        },
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
