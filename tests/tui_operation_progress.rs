use edpcli::application::progress::{
    LogPolicy, OperationKind, OperationRunState, OverallProgress, Phase, ProgressEvent,
    ProgressSpan, Severity, Step, Unit, WorkProgress,
};

fn event(
    overall_basis_points: u16,
    current: u64,
    total: u64,
    log_policy: LogPolicy,
    severity: Severity,
) -> ProgressEvent {
    ProgressEvent {
        operation: OperationKind::Provision,
        phase: Phase::Transaction,
        step: Step::ProtocolWrite,
        overall: OverallProgress::from_basis_points(overall_basis_points),
        stage: None,
        work: Some(WorkProgress::new(current, total, Unit::Sectors)),
        detail: Some(format!("sector {current}/{total}")),
        severity,
        log_policy,
        emitted_at: std::time::Instant::now(),
    }
}

#[test]
fn overall_progress_interpolates_current_work() {
    let span = ProgressSpan::new(2_000, 6_500);
    assert_eq!(span.interpolate(100, 1_000).basis_points(), 2_450);
}

#[test]
fn overall_progress_is_monotonic_and_reaches_100_percent() {
    let mut run = OperationRunState::new(OperationKind::Provision, "disk6");
    run.push(event(4_000, 4, 10, LogPolicy::SnapshotOnly, Severity::Info));
    run.push(event(3_500, 5, 10, LogPolicy::SnapshotOnly, Severity::Info));
    assert_eq!(run.latest.as_ref().unwrap().overall.basis_points(), 4_000);

    let mut completed = event(10_000, 10, 10, LogPolicy::Append, Severity::Info);
    completed.phase = Phase::Complete;
    completed.step = Step::Completed;
    run.push(completed);
    assert_eq!(run.latest.as_ref().unwrap().overall.basis_points(), 10_000);
}

#[test]
fn overall_progress_does_not_divide_by_zero() {
    let span = ProgressSpan::new(2_000, 6_500);
    assert_eq!(span.interpolate(0, 0).basis_points(), 2_000);
}

#[test]
fn sector_progress_updates_snapshot_without_appending_log() {
    let mut run = OperationRunState::new(OperationKind::Provision, "disk6");
    for current in 1..=1_000 {
        run.push(event(
            2_000 + ((4_500 * current) / 1_000) as u16,
            current,
            1_000,
            LogPolicy::SnapshotOnly,
            Severity::Info,
        ));
    }
    let latest = run.latest.as_ref().expect("latest progress");
    assert_eq!(latest.work.as_ref().unwrap().current, 1_000);
    assert_eq!(latest.work.as_ref().unwrap().total, 1_000);
    assert!(run.log.len() < 20);
}

#[test]
fn warning_error_and_rollback_are_never_coalesced_away() {
    let mut run = OperationRunState::new(OperationKind::Provision, "disk6");
    run.push(event(
        5_000,
        1,
        10,
        LogPolicy::SnapshotOnly,
        Severity::Warning,
    ));
    run.push(event(
        5_000,
        2,
        10,
        LogPolicy::SnapshotOnly,
        Severity::Error,
    ));
    let mut rollback = event(5_000, 3, 10, LogPolicy::Append, Severity::Warning);
    rollback.detail = Some("rollback start".into());
    run.push(rollback);
    assert_eq!(run.log.len(), 3);
}

#[test]
fn backup_restore_provision_share_operation_progress_renderer() {
    let shared = include_str!("../src/tui/operation_progress_render.rs");
    let root = include_str!("../src/tui/render.rs");
    let provision = include_str!("../src/tui/provision/render.rs");
    assert!(shared.contains("draw_operation_progress"));
    assert!(root.contains("draw_operation_progress"));
    assert!(!provision.contains("draw_provision_running"));
}

#[test]
fn overall_and_work_gauges_share_high_contrast_progress_labels() {
    let theme = include_str!("../src/tui/theme.rs");
    let overall = include_str!("../src/tui/operation_progress_render.rs");
    let work = include_str!("../src/tui/operation_progress_status.rs");
    assert!(theme.contains("fn progress_label"));
    assert!(theme.contains("self.palette.text_primary"));
    assert!(overall.contains("theme.progress_label()"));
    assert!(work.contains("theme.progress_label()"));
}

#[test]
fn running_progress_uses_four_layers_one_line_safety_and_no_duplicate_global_status() {
    let progress = include_str!("../src/tui/operation_progress_render.rs");
    let current = include_str!("../src/tui/operation_progress_status.rs");
    let root = include_str!("../src/tui/render.rs");
    let status = include_str!("../src/tui/status.rs");

    for required in ["总体进度", "运行记录", "Constraint::Length(1)"] {
        assert!(progress.contains(required), "missing {required}");
    }
    assert!(current.contains("当前任务"));
    assert!(current.contains("工作进度"));
    assert!(current.contains("动态描述"));
    assert!(current.contains("最近活动"));
    assert!(!progress.contains("运行日志 · 最近语义活动"));
    assert!(!current.contains("当前状态"));
    assert!(!progress.contains("扇区活动"));
    assert!(!current.contains("扇区活动"));
    assert!(
        root.contains("let footer_height =")
            && root.contains("!operation_progress_running || notice.is_some() || status.is_some()")
    );
    assert!(
        status.contains("if operation_progress_running") && status.contains("return None;"),
        "Running must not emit a duplicate dynamic safety status"
    );
}

#[test]
fn result_report_pages_keep_static_primitive_and_provision_uses_interactive_workbench() {
    let shared = include_str!("../src/tui/ui/operation_result.rs");
    let workbench = include_str!("../src/tui/result_workbench.rs");
    let provision = include_str!("../src/tui/provision/result_render.rs");
    let provision_layout = include_str!("../src/tui/provision/result_partition_layout.rs");
    let provision_root = include_str!("../src/tui/provision/render.rs");
    let wizard = include_str!("../src/tui/wizard_result_render.rs");

    assert!(shared.contains("pub fn render_operation_result"));
    assert!(wizard.contains("render_operation_result"));
    assert!(workbench.contains("render_result_workbench_shell"));
    assert!(provision.contains("render_result_workbench_shell"));
    assert!(!provision.contains("render_operation_result"));
    assert!(
        !std::path::Path::new("src/tui/backups/result_render.rs").exists(),
        "notification-only backup result pages must not return"
    );
    let provision_result_sources = format!("{provision}\n{provision_layout}");
    for required in ["分区结果", "全盘布局", "验收与执行", "备份文件", "总耗时"]
    {
        assert!(
            provision_result_sources.contains(required),
            "missing {required}"
        );
    }
    let execution = include_str!("../src/tui/provision/execution_state.rs");
    let result_model = include_str!("../src/tui/provision/result_model.rs");
    let result_geometry = include_str!("../src/tui/provision/result_geometry.rs");
    let result_interaction = include_str!("../src/tui/provision/result_interaction.rs");
    let write_section = execution
        .split("pub fn provision_take_for_write")
        .nth(1)
        .expect("write lifecycle section")
        .split("pub fn provision_finish_write")
        .next()
        .expect("write lifecycle boundary");
    assert!(write_section.contains("self.provision.prepared.take()?"));
    assert!(write_section.contains("ProvisionResultSnapshot::from_prepared"));
    assert!(!write_section.contains(".clone()"));
    for secret_marker in [
        "SecretBytes",
        "source_password",
        "file_key",
        "lba12_material",
    ] {
        for source in [result_model, result_geometry, result_interaction, workbench] {
            assert!(
                !source.contains(secret_marker),
                "result workbench state must not retain {secret_marker}"
            );
        }
    }
    assert!(!provision_root.contains("最近进度事件"));
    assert!(!provision.contains("最近进度事件"));
}

#[test]
fn provision_waiting_states_are_overlays_not_full_pages() {
    let provision = include_str!("../src/tui/provision/render.rs");
    let stage_render = provision
        .split("match provision.stage {")
        .nth(1)
        .expect("provision stage render match");
    let planning = stage_render
        .split("ProvisionStage::Planning =>")
        .nth(1)
        .expect("planning render branch")
        .split("ProvisionStage::Review =>")
        .next()
        .expect("planning render boundary");
    assert!(planning.contains("draw_provision_form(frame, main_area, state)"));
    assert!(planning.contains("draw_provision_status_modal"));
    assert!(!planning.contains(".block(crate::tui::ui::card"));

    let exporting = stage_render
        .split("ProvisionStage::Exporting =>")
        .nth(1)
        .expect("exporting render branch")
        .split("ProvisionStage::Confirm =>")
        .next()
        .expect("exporting render boundary");
    assert!(exporting.contains("draw_provision_review(frame, main_area, state)"));
    assert!(exporting.contains("draw_provision_status_modal"));
    assert!(!exporting.contains(".block(crate::tui::ui::card"));
    assert!(
        provision.contains("centered_modal_rect(frame.area(), 76, height)"),
        "Provision waiting overlays must be centered from the full terminal viewport"
    );

    let backups = include_str!("../src/tui/backups/render.rs");
    assert!(
        backups.contains("centered_modal_rect(frame.area(), 78, height)"),
        "Backup waiting overlays must be centered from the full terminal viewport"
    );
    assert!(!provision.contains(
        "draw_provision_status_modal(\n                frame,\n                main_area,"
    ));
    assert!(!backups
        .contains("draw_backup_status_modal(\n                frame,\n                area,"));
}

#[test]
fn progress_transport_coalesces_snapshots_and_limits_render_rate() {
    let task = include_str!("../src/tui/task.rs");
    let transport = include_str!("../src/tui/progress_transport.rs");
    let runtime = include_str!("../src/tui/mod.rs");
    assert!(task.contains("push_progress_coalesced"));
    assert!(transport.contains("can_coalesce"));
    assert!(transport.contains("Severity::Warning"));
    assert!(transport.contains("Severity::Error"));
    assert!(transport.contains("RollbackWrite"));
    assert!(runtime.contains("MIN_RENDER_INTERVAL: Duration = Duration::from_millis(50)"));
}
