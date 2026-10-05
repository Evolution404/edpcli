use edpcli::tui::{
    animation::MotionMode,
    demo, render,
    state::{AppState, NavCommand},
};
use ratatui::{backend::TestBackend, layout::Size, Terminal};

fn screen(state: &mut AppState, width: u16, height: u16) -> Vec<String> {
    state.set_viewport_size(Size::new(width, height));
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, state)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn desktop_backup_density_columns_and_page_navigation_follow_the_visible_list() {
    let fixture = demo::build_scene("backups").unwrap().backups()[0].clone();
    let vid_pid = edpcli::application::identity::WorkspaceIdentity::from_backup(&fixture)
        .display_cells()[1]
        .clone();
    let rows = (1..=94)
        .map(|index| {
            let mut row = fixture.clone();
            row.index = index;
            row.path = format!("desktop-{index}.edpb").into();
            row.file_name = format!("desktop-{index}.edpb");
            row.display_time = "2026-10-05 08:00".into();
            row.dept = Some("很长的部门名称".repeat(20));
            row
        })
        .collect();
    let mut state = AppState::new();
    state.replace_backups(rows);
    state.navigate(NavCommand::WorkspaceBackups, 20);
    let mut previous = 0;
    for (width, height) in [(160, 50), (200, 60), (240, 72)] {
        state.navigate(NavCommand::Top, 20);
        let lines = screen(&mut state, width, height);
        let visible = lines
            .iter()
            .filter(|line| line.contains("2026-10-05 08:00") && line.contains('┃'))
            .count();
        assert!(
            visible > previous,
            "{width}x{height}: {visible} <= {previous}"
        );
        if width == 200 {
            assert!(visible >= 24, "visible={visible}");
            let heading = lines
                .iter()
                .find(|line| {
                    line.replace(' ', "").contains("序号") && line.replace(' ', "").contains("健康")
                })
                .unwrap();
            assert!(heading.contains("VID:PID"), "{heading}");
            assert!(lines.iter().any(|line| line.contains(&vid_pid)));
        }
        assert_eq!(
            state.workspace_navigation_rows(Size::new(width, height)),
            visible
        );
        state.navigate_in_viewport(NavCommand::PageDown, Size::new(width, height));
        assert_eq!(state.selected(), visible);
        previous = visible;
    }
}

#[test]
fn idle_redraw_policy_preserves_active_elapsed_updates_with_motion_off() {
    use std::time::Duration;
    assert_eq!(
        MotionMode::Full.redraw_interval(false),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        MotionMode::Full.redraw_interval(true),
        MotionMode::Full.tick_interval()
    );
    assert_eq!(
        MotionMode::Reduced.redraw_interval(false),
        Some(Duration::from_secs(2))
    );
    assert_eq!(MotionMode::Off.redraw_interval(false), None);
    assert_eq!(
        MotionMode::Off.redraw_interval(true),
        Some(Duration::from_secs(1))
    );
}

#[test]
fn desktop_review_spatial_navigation_matches_stacked_layout() {
    use edpcli::tui::pane::PaneId;
    let mut state = demo::build_scene("provision-review").unwrap();
    state.provision_spatial_focus(0, -1);
    assert_eq!(state.provision_focused_pane(), PaneId::ProvisionDiskLayout);
    state.provision_spatial_focus(0, 1);
    assert_eq!(
        state.provision_focused_pane(),
        PaneId::ProvisionPartitionPlan
    );
    state.provision_spatial_focus(0, 1);
    assert_eq!(
        state.provision_focused_pane(),
        PaneId::ProvisionExecutionSummary
    );
    state.provision_spatial_focus(0, -1);
    assert_eq!(
        state.provision_focused_pane(),
        PaneId::ProvisionPartitionPlan
    );
}

#[test]
fn desktop_single_device_list_does_not_reserve_half_the_window() {
    let mut state = demo::build_scene("devices").unwrap();
    let row = state.devices().iter().find(|row| row.disk == 6).unwrap();
    // Row has no Clone: a fresh fixture has the same single-device behavior after
    // applying a search filter, without needing to duplicate a live device.
    assert_eq!(row.disk, 6);
    state.navigate(NavCommand::Search, 20);
    for ch in "disk6".chars() {
        state.push_input_char(ch);
    }
    state.submit_search();
    let lines = screen(&mut state, 200, 60);
    let tree_top = lines
        .iter()
        .position(|line| line.replace(' ', "").contains("设备信息"))
        .unwrap();
    assert!(tree_top < 16, "tree_top={tree_top}");
}

#[test]
fn desktop_result_fixtures_use_typed_partial_reports_and_do_not_claim_unconfirmed_writes() {
    use edpcli::application::provision::ProvisionExecutionStatus as Status;
    use edpcli::tui::table_layout::TableKind;
    for (scene, status) in [
        ("provision-result-success", Status::Success),
        ("provision-result-warning", Status::CompletedWithWarnings),
        ("provision-result-partial", Status::PartialFormatFailure),
        ("provision-result-failure", Status::FatalFailure),
        ("provision-result-rollback-failure", Status::FatalFailure),
    ] {
        let mut state = demo::build_scene(scene).unwrap();
        assert_eq!(state.provision().result_status, Some(status));
        let view = state.result_partition_table_view().unwrap();
        if status == Status::FatalFailure {
            assert!(view.rows.iter().all(|row| row[5] == "未确认"));
        } else {
            let outcome = state.provision().result_outcome.as_ref().unwrap();
            assert_eq!(outcome.execution_status(), status);
            if status == Status::PartialFormatFailure {
                assert!(view.rows.iter().any(|row| row[5] == "格式化失败"));
                assert!(view.rows.iter().any(|row| row[5] == "已格式化 · 读回通过"));
            }
        }
        let lines = screen(&mut state, 200, 60);
        let compact = lines.join("\n").replace(' ', "");
        assert!(compact.contains("后续处理"));
        let pane_bottom = &lines[58];
        assert!(
            pane_bottom.starts_with('└') || pane_bottom.starts_with('┗'),
            "result partition pane must fill the content height: {pane_bottom}"
        );
        if scene.contains("rollback") {
            assert!(compact.contains("回滚读回失败"));
        }
        if status == Status::PartialFormatFailure {
            assert!(compact.contains("文件系统读回不一致"));
        }
        let copied = state.table_copy_payload(TableKind::ResultPartitions, true);
        assert!(copied.is_some());
    }
}

#[test]
fn desktop_progress_log_fills_workspace_and_keeps_write_safety_at_the_bottom() {
    for scene in ["provision-running", "provision-running-long"] {
        let mut state = demo::build_scene(scene).unwrap();
        let lines = screen(&mut state, 200, 60);
        assert!(lines.last().unwrap().replace(' ', "").contains("安全提示"));
        assert!(lines.last().unwrap().contains("Ctrl-C"));
        let log_top = lines
            .iter()
            .position(|line| line.replace(' ', "").contains("运行记录"))
            .unwrap();
        assert!(log_top < 20);
        let bottom = lines
            .iter()
            .enumerate()
            .skip(log_top + 1)
            .find(|(_, line)| line.contains('└') || line.contains('┗'))
            .unwrap()
            .0;
        assert_eq!(
            bottom,
            lines.len() - 2,
            "progress log card must own all space above the safety footer"
        );
    }
}
