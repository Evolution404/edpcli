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
    let provision_result = fixtures::provision_result_snapshot(&edp);
    let provision_review = fixtures::provision_review_projection(&edp);
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
            state.advanced_inspect_focus_pane(PaneId::InspectDetail);
            if name == "inspect-elabel-expanded" {
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
            let _ = state.begin_provision_for_selected_device();
        }
        name if name.starts_with("provision-") => {
            let _ = state.begin_provision_for_selected_device();
            state.provision_begin_selected();
            state.provision_enter_form_workspace();
            state.provision_mut().stage = match name {
                "provision-form" => ProvisionStage::Form,
                "provision-review" => ProvisionStage::Review,
                "provision-running" | "provision-running-long" => ProvisionStage::Running,
                _ => ProvisionStage::Result,
            };
            let stage = state.provision().stage;
            if stage == ProvisionStage::Review {
                state.provision_mut().review_projection = Some(provision_review.clone());
            }
            if stage == ProvisionStage::Result {
                state.provision_mut().result_plan = Some(provision_result.clone());
                state.provision_initialize_result_workbench();
            }
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
            state.provision_mut().message =
                Some(crate::tui::ui::UiMessage::info("演示模式不会执行真实操作"));
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
        "error-state" => state.set_error_notice("DEMO 错误：模拟读取失败，不访问真实介质"),
        _ => {}
    }
    Ok(state)
}

fn select_lba8(state: &mut AppState) -> bool {
    for suffix in ["/region.protocol", "/sector.8"] {
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
        if let Err(error) = session.terminal.draw(|frame| {
            state.set_viewport_size(frame.area().as_size());
            super::render::draw(frame, &state);
        }) {
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
        let role = super::controller::active_widget_role(&state);
        let Some(action) = keys.map_for_role(state.input_mode(), role, key) else {
            continue;
        };
        let size = session.terminal.size().ok();
        let width = size.map_or(120, |size| size.width);
        let height = size.map_or(24, |size| size.height);
        state.set_viewport_size(ratatui::layout::Size::new(width, height));
        let viewport = if let Some(advanced) = state.advanced_inspect() {
            state
                .advanced_inspect_focused_pane()
                .map(|pane| {
                    crate::tui::inspect_layout::InspectBrowserLayout::from_terminal_size(
                        ratatui::layout::Size::new(width, height),
                        advanced.panel,
                    )
                    .visible_rows(pane)
                })
                .unwrap_or(1)
        } else {
            usize::from(height.saturating_sub(8)).max(1)
        };
        let outcome = super::controller::dispatch_action(
            &mut state,
            action,
            role,
            viewport,
            width,
            &mut super::clipboard::ClipboardService,
        );
        if !outcome.handled {
            continue;
        }
        let mut effect = outcome.effect;
        if let Some(request) = outcome.request {
            effect = execute_demo_request(&mut state, request, viewport);
        }
        hydrate_inspect(&mut state);
        if effect == crate::tui::state::StateEffect::ExitRequested {
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

fn execute_demo_request(
    state: &mut AppState,
    request: super::controller::ActionRequest,
    viewport: usize,
) -> crate::tui::state::StateEffect {
    match request {
        super::controller::ActionRequest::Navigate(NavCommand::OpenInspect) => {
            let effect = state.navigate(NavCommand::WorkspaceInspect, viewport);
            hydrate_inspect(state);
            effect
        }
        super::controller::ActionRequest::Navigate(_) => {
            state.set_notice("演示模式不会执行真实外部操作");
            crate::tui::state::StateEffect::None
        }
        super::controller::ActionRequest::InspectSelection => {
            open_cached_inspect_selection(state);
            crate::tui::state::StateEffect::None
        }
        super::controller::ActionRequest::InspectPreview { lba, .. } => {
            state.advanced_inspect_sector_finish(lba, Err("演示模式不会读取真实扇区".into()));
            crate::tui::state::StateEffect::None
        }
        super::controller::ActionRequest::ProvisionKeyProbe { .. } => {
            state.provision_mut().message = Some(crate::tui::ui::UiMessage::info(
                "演示模式使用固定制盘夹具，不探测真实介质密钥。",
            ));
            crate::tui::state::StateEffect::None
        }
        super::controller::ActionRequest::ProvisionPlan => {
            state.set_notice("演示模式不会启动真实后台任务");
            crate::tui::state::StateEffect::None
        }
    }
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
        let cell = super::super::controller::dispatch_action(
            &mut state,
            TuiAction::TableCopyCell,
            super::super::keymap::WidgetRole::Table,
            28,
            120,
            &mut clipboard,
        );
        assert!(cell.handled);
        let row = super::super::controller::dispatch_action(
            &mut state,
            TuiAction::TableCopyRow,
            super::super::keymap::WidgetRole::Table,
            28,
            120,
            &mut clipboard,
        );
        assert!(row.handled);
        assert_eq!(clipboard.0.len(), 2);
        assert_eq!(clipboard.0[0], "disk6");
        assert!(clipboard.0[1].contains("DEMO"));
        assert_eq!(clipboard.0[1].split('\t').count(), 11);
    }
}
