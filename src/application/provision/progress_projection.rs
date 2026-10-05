use crate::application::progress::{
    LogPolicy, Phase, ProgressEvent, ProgressSpan, Severity, Step, TransactionActivityPhase,
    WorkProgress,
};
use crate::diskio::TransactionActivity;

pub(super) fn commit_event(
    phase: Phase,
    step: Step,
    current: u64,
    total: u64,
    activity: Option<TransactionActivity>,
) -> ProgressEvent {
    let mut event = ProgressEvent::new(phase, step, current, total);
    if let Some(activity) = activity {
        let work = WorkProgress::from_activity(activity);
        let step_span = ProgressSpan::for_index(current, total);
        let work_span = match activity.phase {
            TransactionActivityPhase::SyncPreflight => step_span.subspan(0, 0),
            TransactionActivityPhase::Mirror => step_span.subspan(0, 1_000),
            TransactionActivityPhase::Write => step_span.subspan(1_000, 6_000),
            TransactionActivityPhase::Sync => step_span.subspan(6_000, 6_000),
            TransactionActivityPhase::Readback => step_span.subspan(6_000, 10_000),
            TransactionActivityPhase::FormatWrite => step_span.subspan(0, 7_500),
            TransactionActivityPhase::FormatReadback => step_span.subspan(7_500, 10_000),
            TransactionActivityPhase::RollbackWrite
            | TransactionActivityPhase::RollbackSync
            | TransactionActivityPhase::RollbackReadback => step_span.subspan(0, 0),
        };
        event.overall = work_span.interpolate(work.current, work.total);
        if activity.total > 0 {
            event.delivery = crate::application::progress::ProgressDelivery::WorkSnapshot;
            event.work = Some(work);
            event.log_policy = LogPolicy::SnapshotOnly;
        } else {
            event.detail = Some(
                match activity.phase {
                    TransactionActivityPhase::SyncPreflight => "写前缓存同步预检",
                    TransactionActivityPhase::RollbackSync => "同步回滚缓存到介质",
                    _ => "同步写入缓存到介质",
                }
                .into(),
            );
            event.log_policy = LogPolicy::AppendOnChange;
        }
        if matches!(
            activity.phase,
            TransactionActivityPhase::RollbackWrite
                | TransactionActivityPhase::RollbackSync
                | TransactionActivityPhase::RollbackReadback
        ) {
            event.severity = Severity::Warning;
        }
    } else {
        event.log_policy = LogPolicy::AppendOnChange;
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diskio::TransactionActivity;

    #[test]
    fn transaction_activity_is_snapshot_only_but_step_completion_is_a_milestone() {
        let activity = TransactionActivity {
            phase: TransactionActivityPhase::FormatWrite,
            current: 128,
            total: 512,
        };
        let snapshot = commit_event(
            Phase::Format,
            Step::PartitionFormat(crate::provision::PartitionRole::Boot),
            4,
            8,
            Some(activity),
        );
        assert_eq!(snapshot.log_policy, LogPolicy::SnapshotOnly);
        assert!(snapshot.work.is_some());

        let milestone = commit_event(
            Phase::Format,
            Step::PartitionFormat(crate::provision::PartitionRole::Boot),
            5,
            8,
            None,
        );
        assert_eq!(milestone.log_policy, LogPolicy::AppendOnChange);
        assert!(milestone.work.is_none());
    }

    #[test]
    fn rollback_snapshot_keeps_warning_severity_for_safety_log() {
        let rollback = commit_event(
            Phase::Transaction,
            Step::ProtocolWrite,
            3,
            8,
            Some(TransactionActivity {
                phase: TransactionActivityPhase::RollbackWrite,
                current: 1,
                total: 13,
            }),
        );
        assert_eq!(rollback.log_policy, LogPolicy::SnapshotOnly);
        assert_eq!(rollback.severity, Severity::Warning);
    }
}
