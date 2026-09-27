//! Deterministic, in-memory scenes for the production TUI renderer.

use std::time::Duration;

use crate::common::{EXIT_IO, EXIT_OK, EXIT_USAGE};
use crate::tui::pane::PaneId;
use crate::tui::state::{AdvancedInspectSource, AppState, NavCommand, ProvisionStage};

mod fixtures;
pub mod timeline;

pub const SCENES: &[&str] = &[
    "overview",
    "devices",
    "device-edp",
    "device-plain",
    "inspect-lba8",
    "inspect-elabel-expanded",
    "inspect-disk-layout",
    "inspect-tail-expanded",
    "inspect-sector-raw",
    "inspect-sector-decode",
    "inspect-sector-meta",
    "provision-select",
    "provision-form",
    "provision-review",
    "provision-running",
    "provision-running-long",
    "provision-result-success",
    "provision-result-warning",
    "provision-result-failure",
    "backups",
    "backup-detail",
    "backup-coverage",
    "backup-verify-running",
    "empty-state",
    "error-state",
];

pub fn build_scene(scene: &str) -> Result<AppState, String> {
    if !SCENES.contains(&scene) {
        return Err(format!("未知演示场景 {scene}"));
    }
    let mut state = AppState::new();
    state.set_demo_mode();
    if scene == "empty-state" {
        return Ok(state);
    }
    let edp = fixtures::disk(6, crate::provision::DiskProvisionKind::Mode0);
    let edp_mode1 = fixtures::disk(7, crate::provision::DiskProvisionKind::Mode1);
    let plain = fixtures::disk(8, crate::provision::DiskProvisionKind::Plain);
    let inspect_fixture = fixtures::inspect_workspace(&edp);
    let backups = vec![
        fixtures::backup(1, "confirmed.edpb", true, &edp, true),
        fixtures::backup(2, "possible-health-warning.edpb", false, &edp, false),
    ];
    state.replace_devices(vec![edp, edp_mode1, plain]);
    state.replace_backups(backups);

    match scene {
        "overview" | "devices" | "device-edp" => {}
        "device-plain" => {
            state.navigate(NavCommand::Down, 20);
            state.navigate(NavCommand::Down, 20);
        }
        name if name.starts_with("inspect-") => {
            state.begin_advanced_inspect(AdvancedInspectSource::Disk(6));
            state.advanced_inspect_finish(Ok(inspect_fixture));
            let _ = select_lba8(&mut state);
            if name.contains("disk-layout") || name.contains("tail-expanded") {
                state.advanced_inspect_focus_pane(PaneId::InspectDiskLayout);
            } else {
                state.advanced_inspect_focus_pane(PaneId::InspectDetail);
            }
            if name == "inspect-tail-expanded" {
                state.toggle_disk_layout_tail();
            } else if name == "inspect-elabel-expanded" {
                state.pane_viewport_mut(PaneId::InspectDetail).selected = Some(1);
                state.advanced_inspect_detail_toggle_selected();
            } else if name.starts_with("inspect-sector-") {
                let _ = state.advanced_inspect_open_selected_sector();
                let mode = match name {
                    "inspect-sector-raw" => crate::tui::state::SectorInspectMode::Raw,
                    "inspect-sector-decode" => crate::tui::state::SectorInspectMode::Decode,
                    _ => crate::tui::state::SectorInspectMode::Mixed,
                };
                state.advanced_inspect_sector_set_mode(mode);
            }
        }
        "provision-select" => {
            state.navigate(NavCommand::WorkspaceProvision, 20);
        }
        name if name.starts_with("provision-") => {
            state.navigate(NavCommand::WorkspaceProvision, 20);
            state.provision_select_disk();
            state.provision_begin_selected();
            state.provision_mut().stage = match name {
                "provision-form" => ProvisionStage::Form,
                "provision-review" => ProvisionStage::Review,
                "provision-running" | "provision-running-long" => ProvisionStage::Running,
                _ => ProvisionStage::Result,
            };
            let stage = state.provision().stage;
            state.provision_mut().pane_focus = match stage {
                ProvisionStage::Review => crate::tui::pane::PaneFocus::provision_review(),
                ProvisionStage::Running => crate::tui::pane::PaneFocus::provision_running(),
                _ => crate::tui::pane::PaneFocus::provision_form(),
            };
            if name.ends_with("success") {
                state.provision_mut().result_status =
                    Some(crate::application::provision::ProvisionExecutionStatus::Success);
            } else if name.ends_with("warning") {
                state.provision_mut().result_status = Some(
                    crate::application::provision::ProvisionExecutionStatus::CompletedWithWarnings,
                );
            } else if name.ends_with("failure") {
                state.provision_mut().result_status =
                    Some(crate::application::provision::ProvisionExecutionStatus::FatalFailure);
            }
            state.provision_mut().message = Some("演示模式不会执行真实操作".into());
            if name == "provision-running-long" {
                let tick = timeline::DemoTimeline::LONG_INITIAL_TICK;
                state.provision_mut().run = Some(timeline::DemoTimeline::long_at_tick(
                    tick,
                    std::time::Instant::now() - Duration::from_secs(tick as u64),
                ));
            } else if matches!(stage, ProvisionStage::Running | ProvisionStage::Result) {
                let tick = if stage == ProvisionStage::Running {
                    3
                } else {
                    5
                };
                state.provision_mut().run = Some(timeline::DemoTimeline::at_tick(
                    tick,
                    std::time::Instant::now() - Duration::from_secs(10),
                ));
            }
        }
        "backups" | "backup-detail" | "backup-coverage" | "backup-verify-running" => {
            state.navigate(NavCommand::WorkspaceBackups, 20);
            if scene == "backup-detail" {
                state.focus_backups_pane(PaneId::BackupSummary);
            } else if scene == "backup-coverage" {
                state.focus_backups_pane(PaneId::BackupCoverage);
            } else if scene == "backup-verify-running" {
                state.set_backup_verify_run(Some(timeline::DemoTimeline::backup_verify_at_tick(
                    1,
                    std::time::Instant::now() - Duration::from_secs(10),
                )));
                state.set_notice("演示模式不会执行真实操作");
            }
        }
        "error-state" => state.set_notice("DEMO 错误：模拟读取失败，不访问真实介质"),
        _ => {}
    }
    Ok(state)
}

fn select_lba8(state: &mut AppState) -> bool {
    for suffix in ["/region.protocol", "/region.protocol.extent", "/sector.8"] {
        let rows = state.advanced_inspect_tree_rows();
        let Some(index) = rows.iter().position(|row| row.id.ends_with(suffix)) else {
            return false;
        };
        let current = state
            .advanced_inspect()
            .map_or(0, |advanced| advanced.tree_selected);
        state.advanced_inspect_move_tree(index as isize - current as isize);
        if suffix != "/sector.8" {
            state.advanced_inspect_toggle_selected();
        }
    }
    true
}

pub fn run(scene: Option<&str>, list_scenes: bool) -> i32 {
    if list_scenes {
        for scene in SCENES {
            println!("{scene}");
        }
        return EXIT_OK;
    }
    let scene = scene.unwrap_or("overview");
    if !SCENES.contains(&scene) {
        eprintln!("错误: 未知演示场景 {scene}；可用 edpcli demo --list-scenes 查看。 ");
        return EXIT_USAGE;
    }
    crate::tui::execution::launch(
        crate::tui::execution::ExecutionPolicy::DemoNoExternalIo,
        scene,
    )
}

/// This loop deliberately owns no TaskHub, runner, backup directory, or elevation path.
/// Every scene is built from typed in-memory state and drawn by the production renderer.
pub(super) fn run_interactive(scene: &str) -> i32 {
    if !super::is_interactive_terminal() {
        eprintln!("错误: edpcli demo 需要交互式 TTY；场景列表可使用 --list-scenes。");
        return EXIT_USAGE;
    }
    let mut state = match build_scene(scene) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("错误: {error}");
            return EXIT_USAGE;
        }
    };
    let mut session = match super::TerminalSession::enter() {
        Ok(session) => session,
        Err(error) => {
            eprintln!("错误: 演示终端初始化失败: {error}");
            return EXIT_IO;
        }
    };
    let mut keys = super::KeyMapper::new();
    let timeline_base = std::time::Instant::now()
        - Duration::from_secs(if scene == "provision-running-long" {
            timeline::DemoTimeline::LONG_INITIAL_TICK as u64
        } else {
            10
        });
    loop {
        if let Err(error) = session
            .terminal
            .draw(|frame| super::render::draw(frame, &state))
        {
            eprintln!("错误: 演示绘制失败: {error}");
            return EXIT_IO;
        }
        match super::ct_event::poll(Duration::from_millis(50)) {
            Ok(false) => {
                state.advance_animation();
                advance_scene_timeline(&mut state, scene, timeline_base);
                continue;
            }
            Ok(true) => {}
            Err(error) => {
                eprintln!("错误: 演示终端事件失败: {error}");
                return EXIT_IO;
            }
        }
        let event = match super::ct_event::read() {
            Ok(event) => event,
            Err(error) => {
                eprintln!("错误: 演示终端输入失败: {error}");
                return EXIT_IO;
            }
        };
        let super::ct_event::Event::Key(key) = event else {
            continue;
        };
        let role = if state.active_table_kind().is_some() {
            super::keymap::WidgetRole::Table
        } else {
            super::keymap::WidgetRole::Other
        };
        let Some(action) = keys.map_for_role(state.input_mode(), role, key) else {
            continue;
        };
        let size = session.terminal.size().ok();
        if handle_action(
            &mut state,
            action,
            size.map_or(120, |size| size.width),
            size.map_or(24, |size| size.height),
            &mut super::clipboard::ClipboardService,
        ) {
            break;
        }
    }
    EXIT_OK
}

fn advance_scene_timeline(state: &mut AppState, scene: &str, base: std::time::Instant) {
    let step = (state.animation_frame() / 20) as usize;
    match scene {
        "provision-running" => {
            let tick = (3 + step) % (timeline::DemoTimeline::LAST_TICK + 1);
            state.provision_mut().run = Some(timeline::DemoTimeline::at_tick(tick, base));
        }
        "provision-running-long" => {
            let tick = (timeline::DemoTimeline::LONG_INITIAL_TICK + step)
                .min(timeline::DemoTimeline::LONG_LAST_TICK);
            state.provision_mut().run = Some(timeline::DemoTimeline::long_at_tick(tick, base));
        }
        "backup-verify-running" => {
            let tick = step % (timeline::DemoTimeline::BACKUP_LAST_TICK + 1);
            state.set_backup_verify_run(Some(timeline::DemoTimeline::backup_verify_at_tick(
                tick, base,
            )));
        }
        _ => {}
    }
}

fn hydrate_inspect(state: &mut AppState) {
    if state.workspace() == crate::tui::state::Workspace::Inspect
        && state.advanced_inspect().is_none()
    {
        let row = fixtures::disk(6, crate::provision::DiskProvisionKind::Mode0);
        let fixture = fixtures::inspect_workspace(&row);
        state.begin_advanced_inspect(AdvancedInspectSource::Disk(6));
        state.advanced_inspect_finish(Ok(fixture));
    }
}

fn handle_action(
    state: &mut AppState,
    action: super::keymap::TuiAction,
    width: u16,
    height: u16,
    clipboard: &mut dyn super::clipboard::ClipboardBackend,
) -> bool {
    use super::keymap::TuiAction;
    use crate::tui::state::Workspace;

    let viewport = usize::from(height.saturating_sub(8)).max(1);
    match action {
        TuiAction::Quit => return true,
        TuiAction::WorkspaceNext | TuiAction::WorkspacePrevious => {
            let command = if action == TuiAction::WorkspaceNext {
                NavCommand::NextWorkspace
            } else {
                NavCommand::PreviousWorkspace
            };
            state.navigate(command, viewport);
            hydrate_inspect(state);
        }
        TuiAction::MoveUp | TuiAction::MoveDown => {
            let delta = if action == TuiAction::MoveUp { -1 } else { 1 };
            if state.workspace() == Workspace::Inspect {
                let count = state.advanced_inspect_focused_content_len();
                state.advanced_inspect_move_focused_vertical(delta, viewport, count);
            } else if state.workspace() == Workspace::Provision
                && matches!(
                    state.provision().stage,
                    ProvisionStage::Form | ProvisionStage::Review
                )
            {
                let count = state.provision_focused_content_len();
                state.provision_move_focused_vertical(delta, viewport, count);
            } else {
                state.navigate(
                    if delta < 0 {
                        NavCommand::Up
                    } else {
                        NavCommand::Down
                    },
                    viewport,
                );
            }
        }
        TuiAction::MoveLeft if state.workspace() == Workspace::Inspect => {
            state.advanced_inspect_collapse_or_parent();
        }
        TuiAction::MoveRight if state.workspace() == Workspace::Inspect => {
            state.advanced_inspect_expand_or_child();
        }
        TuiAction::Top if state.workspace() == Workspace::Inspect => {
            state.advanced_inspect_focused_top();
        }
        TuiAction::Bottom if state.workspace() == Workspace::Inspect => {
            state.advanced_inspect_focused_bottom();
        }
        TuiAction::TableColumnLeft
        | TuiAction::TableColumnRight
        | TuiAction::TableMoveColumnLeft
        | TuiAction::TableMoveColumnRight
        | TuiAction::TableColumnFirst
        | TuiAction::TableColumnLast
        | TuiAction::TableCopyCell
        | TuiAction::TableCopyRow => {
            let _ = super::table_dispatch::dispatch_table_action_with_clipboard(
                state, action, viewport, width, clipboard,
            );
        }
        TuiAction::Open => {
            if state.advanced_inspect_focused_pane() == Some(PaneId::InspectDiskLayout)
                || (state.workspace() == Workspace::Provision
                    && state.provision_focused_pane() == PaneId::ProvisionDiskLayout)
            {
                state.toggle_disk_layout_tail();
            } else if state.workspace() == Workspace::Inspect {
                super::dispatch::open_inspect_selection(state);
            }
        }
        TuiAction::Activate => match state.workspace() {
            Workspace::Devices => {
                let _ = state.activate_device_for_viewport(width);
            }
            Workspace::Inspect => {
                if state.advanced_inspect_focused_pane() == Some(PaneId::InspectDiskLayout) {
                    super::dispatch::show_inspect_layout_detail(state);
                } else {
                    open_cached_inspect_selection(state);
                }
            }
            Workspace::Provision => match state.provision().stage {
                ProvisionStage::SelectDisk => {
                    state.provision_select_disk();
                }
                ProvisionStage::Menu => {
                    state.provision_begin_selected();
                }
                _ => state.set_notice("演示模式不会执行真实操作"),
            },
            Workspace::Backups => state.focus_backups_pane(PaneId::BackupSummary),
        },
        TuiAction::PanelNext => match state.workspace() {
            Workspace::Inspect => state.advanced_inspect_shift_panel(false),
            Workspace::Provision => state.provision_shift_pane(false),
            Workspace::Devices => state.shift_workspace_pane(false),
            Workspace::Backups => state.shift_workspace_pane(false),
        },
        TuiAction::PanelPrevious => match state.workspace() {
            Workspace::Inspect => state.advanced_inspect_shift_panel(true),
            Workspace::Provision => state.provision_shift_pane(true),
            Workspace::Devices | Workspace::Backups => state.shift_workspace_pane(true),
        },
        TuiAction::Yank | TuiAction::YankRaw if state.workspace() == Workspace::Inspect => {
            if state.advanced_inspect_focused_pane() == Some(PaneId::InspectDetail) {
                let _ = state.advanced_inspect_detail_yank(action == TuiAction::YankRaw);
            }
        }
        TuiAction::Provision => {
            state.navigate(NavCommand::WorkspaceProvision, viewport);
        }
        TuiAction::Back => {
            state.navigate(NavCommand::Escape, viewport);
            hydrate_inspect(state);
        }
        TuiAction::BackupCreate
        | TuiAction::Restore
        | TuiAction::Delete
        | TuiAction::ViewOrVerify
        | TuiAction::Export
        | TuiAction::Refresh => state.set_notice("演示模式不会执行真实操作"),
        _ => {}
    }
    false
}

fn open_cached_inspect_selection(state: &mut AppState) {
    let detail_selected = state.advanced_inspect_focused_pane() == Some(PaneId::InspectDetail)
        && state.advanced_inspect_detail_selected_row().is_some();
    let request = if detail_selected {
        state.advanced_inspect_detail_open_selected()
    } else if state.advanced_inspect_selected_sector_lba().is_some() {
        state.advanced_inspect_open_selected_sector()
    } else {
        state.advanced_inspect_enter_selected();
        None
    };
    if let Some((_source, lba)) = request {
        state.advanced_inspect_sector_finish(lba, Err("演示模式不会读取真实扇区".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::clipboard::{ClipboardBackend, ClipboardOutcome};
    use crate::tui::keymap::TuiAction;

    #[derive(Default)]
    struct CapturingClipboard(Vec<String>);

    impl ClipboardBackend for CapturingClipboard {
        fn copy(&mut self, content: &str) -> ClipboardOutcome {
            self.0.push(content.into());
            ClipboardOutcome::Confirmed
        }
    }

    #[test]
    fn demo_table_actions_forward_cell_and_row_to_shared_clipboard() {
        let mut state = build_scene("devices").unwrap();
        let mut clipboard = CapturingClipboard::default();
        assert!(!handle_action(
            &mut state,
            TuiAction::TableCopyCell,
            120,
            36,
            &mut clipboard,
        ));
        assert!(!handle_action(
            &mut state,
            TuiAction::TableCopyRow,
            120,
            36,
            &mut clipboard,
        ));
        assert_eq!(clipboard.0.len(), 2);
        assert_eq!(clipboard.0[0], "disk6");
        assert!(clipboard.0[1].contains("DEMO"));
        assert_eq!(clipboard.0[1].split('\t').count(), 8);
    }
}
