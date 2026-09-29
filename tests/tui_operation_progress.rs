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
