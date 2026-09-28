use edpcli::tui::{demo, render};
use ratatui::{backend::TestBackend, Terminal};

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
    assert!(run.log.len() >= 40);
    assert!(run.log.iter().any(|event| {
        event.step == Step::ProtocolWrite
            && event.work.is_some_and(|work| {
                work.phase == TransactionActivityPhase::Write
                    && work.current > 0
                    && work.current < work.total
            })
    }));
    for role in [
        PartitionRole::Boot,
        PartitionRole::Share,
        PartitionRole::Encrypt,
    ] {
        assert!(run.log.iter().any(|event| {
            event.step == Step::PartitionFormat(role)
                && event
                    .work
                    .is_some_and(|work| work.phase == TransactionActivityPhase::FormatWrite)
        }));
        assert!(run.log.iter().any(|event| {
            event.step == Step::PartitionFormat(role)
                && event
                    .work
                    .is_some_and(|work| work.phase == TransactionActivityPhase::FormatReadback)
        }));
    }
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
    for expected in ["扇区活动", "DEMO慢盘", "%", "运行日志"] {
        assert!(screen.contains(expected), "{expected}");
    }
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
    for expected in ["阶段", "步骤", "日志", "演示模式不会执行真实操作"] {
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
