use edpcli::tui::{demo, render};
use ratatui::{backend::TestBackend, Terminal};
use unicode_width::UnicodeWidthStr;

fn screen_text(scene: &str) -> String {
    let state = demo::build_scene(scene).unwrap();
    let (width, height) = (160, 45);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn all_demo_scenes_build_and_render_at_supported_sizes() {
    for scene in demo::SCENES {
        let state = demo::build_scene(scene).expect(scene);
        assert!(state.is_demo(), "{scene}");
        for (width, height) in [(40, 10), (80, 24), (120, 36), (160, 45), (240, 60)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| render::draw(frame, &state)).unwrap();
        }
    }
}

#[test]
fn demo_fixtures_expose_typed_inspect_layout_backups_and_running_progress() {
    let inspect = demo::build_scene("inspect-lba8").unwrap();
    assert_eq!(inspect.advanced_inspect_selected_sector_lba(), Some(8));
    assert!(!inspect.advanced_inspect_detail_rows().is_empty());

    let plain = demo::build_scene("device-plain").unwrap();
    let layout = plain.selected_device().unwrap().canonical_layout().unwrap();
    assert!(layout.tail_group().is_none());

    let backups = demo::build_scene("backups").unwrap();
    assert!(backups.backups().len() >= 2);
    let target = backups.selected_device().expect("demo target device");
    let confirmed = edpcli::application::identity::WorkspaceIdentity::from_backup_against(
        &backups.backups()[0],
        Some(target),
    );
    assert_eq!(
        confirmed.canonical.unwrap().relationship,
        edpcli::application::media_identity::MediaRelationship::SamePhysicalMedia
    );
    let possible = edpcli::application::identity::WorkspaceIdentity::from_backup_against(
        &backups.backups()[1],
        Some(target),
    );
    assert_ne!(
        possible.canonical.unwrap().relationship,
        edpcli::application::media_identity::MediaRelationship::SamePhysicalMedia
    );
    assert_ne!(
        backups.backups()[1].integrity_status,
        edpcli::application::BackupIntegrityStatus::Verified
    );

    let coverage = demo::build_scene("backup-coverage").unwrap();
    let coverage = coverage.backups()[0]
        .coverage
        .as_ref()
        .expect("typed demo backup coverage");
    assert!(!coverage.regions.is_empty());
    assert!(
        coverage
            .regions
            .iter()
            .all(|region| region.id != "user-files" && region.role != "普通用户文件"),
        "metadata-only demo coverage must not imply user-file capture"
    );
    assert!(coverage.extent_count > 0);
    assert!(coverage.artifact_count > 0);

    let running = demo::build_scene("provision-running").unwrap();
    let run = running
        .provision()
        .run
        .as_ref()
        .expect("typed demo progress");
    assert!(run.latest.is_some());
    assert!(!run.log.is_empty());

    let backup_verify = demo::build_scene("backup-verify-running").unwrap();
    let verify = backup_verify
        .backup_verify_run()
        .expect("typed backup verification progress");
    assert!(verify.path.to_string_lossy().contains("DEMO"));
    assert!(!verify.log.is_empty());
}

#[test]
fn provision_result_demo_uses_real_result_workbench_state() {
    let state = demo::build_scene("provision-result-success").unwrap();
    assert!(state.provision().result_plan.is_some());
    assert_eq!(
        state.provision().result_workbench.selected_partition,
        Some(0)
    );
    assert!(state
        .provision()
        .result_workbench
        .region_selection()
        .is_some());

    let screen = screen_text("provision-result-success");
    let compact = screen.replace(' ', "");
    assert!(
        screen.contains("P3"),
        "third partition row must stay visible above the scrollbar"
    );
    for expected in [
        "制盘结果",
        "分区结果",
        "当前分区",
        "全盘布局",
        "验收与执行",
        "▲",
    ] {
        assert!(
            compact.contains(expected),
            "missing {expected} in result demo"
        );
    }
}

#[test]
fn provision_result_partition_row_is_contiguous_and_omits_lba_range() {
    let state = demo::build_scene("provision-result-success").unwrap();
    let (width, height) = (160, 45);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render::draw(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();

    let left_limit = 76u16;
    let (p1_x, row_y) = (0..height)
        .find_map(|y| {
            (0..left_limit.saturating_sub(1)).find_map(|x| {
                (buffer[(x, y)].symbol() == "P" && buffer[(x + 1, y)].symbol() == "1")
                    .then_some((x, y))
            })
        })
        .expect("selected P1 row in result partition table");
    let selected_bg = buffer[(p1_x, row_y)].bg;
    let selected_x = (0..left_limit)
        .filter(|x| buffer[(*x, row_y)].bg == selected_bg)
        .collect::<Vec<_>>();
    assert!(
        selected_x.len() > 20,
        "selected result row background is too short"
    );
    let first = *selected_x.first().unwrap();
    let last = *selected_x.last().unwrap();
    for x in first..=last {
        if buffer[(x, row_y)].bg == selected_bg {
            continue;
        }
        let wide_continuation =
            x > first && UnicodeWidthStr::width(buffer[(x - 1, row_y)].symbol()) == 2;
        assert!(
            wide_continuation,
            "selected result row background has a real gap at x={x}, y={row_y}"
        );
    }

    let screen = screen_text("provision-result-success");
    assert!(
        !screen.contains("LBA 范围"),
        "result table must not expose LBA range"
    );
}

#[test]
fn demo_timeline_is_deterministic_at_a_frozen_tick() {
    let base = std::time::Instant::now();
    let first = demo::timeline::DemoTimeline::at_tick(3, base);
    let second = demo::timeline::DemoTimeline::at_tick(3, base);
    assert_eq!(first.latest, second.latest);
    assert_eq!(first.log, second.log);
    assert!(first.latest.unwrap().work.is_some());
}

#[test]
fn long_provision_demo_exercises_slow_protocol_and_partition_progress() {
    use edpcli::application::progress::{Step, TransactionActivityPhase};
    use edpcli::provision::PartitionRole;

    assert!(demo::SCENES.contains(&"provision-running-long"));
    let base = std::time::Instant::now();
    let run = demo::timeline::DemoTimeline::long_at_tick(
        demo::timeline::DemoTimeline::LONG_LAST_TICK,
        base,
    );
    assert_eq!(
        run.log.len(),
        11,
        "demo history should contain milestones only"
    );
    assert!(
        run.log.iter().all(|event| event.work.is_none()),
        "high-frequency work snapshots must stay out of history"
    );
    for role in [
        PartitionRole::Boot,
        PartitionRole::Share,
        PartitionRole::Encrypt,
    ] {
        assert!(run
            .log
            .iter()
            .any(|event| event.step == Step::PartitionFormat(role)));
    }
    let running = demo::timeline::DemoTimeline::long_at_tick(
        demo::timeline::DemoTimeline::LONG_INITIAL_TICK,
        base,
    );
    assert_eq!(running.log.len(), 6);
    assert!(running.latest.is_some_and(|event| {
        event.step == Step::PartitionFormat(PartitionRole::Boot)
            && event
                .work
                .is_some_and(|work| work.activity == Some(TransactionActivityPhase::FormatWrite))
    }));
    assert!(run
        .log
        .iter()
        .filter_map(|event| event.detail.as_deref())
        .any(|detail| detail.contains("DEMO 慢盘")));
}

#[test]
fn long_provision_scene_opens_running_page_with_nested_activity_visible() {
    let state = demo::build_scene("provision-running-long").unwrap();
    assert_eq!(
        state.provision().stage,
        edpcli::tui::state::ProvisionStage::Running
    );
    let run = state.provision().run.as_ref().expect("long demo progress");
    assert!(run.log.len() > 5);
    let screen = screen_text("provision-running-long").replace(' ', "");
    for expected in ["当前任务", "DEMO慢盘", "%", "运行记录"] {
        assert!(screen.contains(expected), "{expected}");
    }
    assert!(!screen.contains("扇区活动"), "{screen}");
}

#[test]
fn demo_scenes_show_real_workspace_content_and_safety_header() {
    for scene in ["devices", "inspect-lba8", "provision-running", "backups"] {
        let screen = screen_text(scene);
        let compact = screen.replace(' ', "");
        assert!(compact.contains("演示模式"), "{scene}");
        assert!(compact.contains("不访问真实介质"), "{scene}");
        assert!(screen.contains("DEMO"), "{scene}");
    }
    let running = screen_text("provision-running").replace(' ', "");
    for expected in ["阶段", "当前步骤", "运行记录", "演示模式不会执行真实操作"]
    {
        assert!(running.contains(expected), "{expected}");
    }
    let backup_verify = screen_text("backup-verify-running").replace(' ', "");
    for expected in ["备份校验进行中", "当前阶段", "运行日志"] {
        assert!(backup_verify.contains(expected), "{expected}");
    }
}

#[test]
fn demo_execution_policy_rejects_external_io() {
    use edpcli::tui::execution::ExecutionPolicy;
    use std::cell::Cell;
    assert!(ExecutionPolicy::Live.permits_external_io());
    assert!(!ExecutionPolicy::DemoNoExternalIo.permits_external_io());
    let calls = Cell::new(0);
    for operation in [
        "device scan",
        "backup directory scan",
        "raw disk open",
        "backup mutation",
        "provision",
        "sudo",
        "external shell",
    ] {
        assert_eq!(
            ExecutionPolicy::DemoNoExternalIo.run_external(|| {
                calls.set(calls.get() + 1);
                operation
            }),
            None,
            "{operation}"
        );
    }
    assert_eq!(calls.get(), 0);
    assert_eq!(ExecutionPolicy::Live.run_external(|| 42), Some(42));
}

#[test]
fn demo_loop_has_no_external_task_service_or_process_entry_point() {
    let sources = [
        include_str!("../src/tui/demo/mod.rs"),
        include_str!("../src/tui/demo/fixtures.rs"),
        include_str!("../src/tui/demo/timeline.rs"),
    ];
    for source in sources {
        for forbidden in [
            "TaskHub::",
            "tasks.request_",
            "std::fs::",
            "std::process::",
            "OpenOptions::",
            "elevate::",
            "diskutil",
            "copy_osc52_to",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
}
