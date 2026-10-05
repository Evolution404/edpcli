use crate::application::progress::{LogPolicy, ProgressEvent, Severity};

pub(super) fn can_coalesce(event: &ProgressEvent) -> bool {
    event.delivery == crate::application::progress::ProgressDelivery::WorkSnapshot
        && event.work.is_some()
        && event.severity != Severity::Error
        && event.log_policy != LogPolicy::Append
}

pub(super) fn same_snapshot_stream(left: &ProgressEvent, right: &ProgressEvent) -> bool {
    left.operation == right.operation
        && left.phase == right.phase
        && left.step == right.step
        && left.work.and_then(|work| work.activity) == right.work.and_then(|work| work.activity)
}

pub(super) fn push_progress_coalesced<T: Copy + Eq>(
    updates: &mut Vec<(T, ProgressEvent)>,
    operation_id: T,
    event: ProgressEvent,
) {
    if can_coalesce(&event) {
        if let Some((last_operation_id, last_event)) = updates.last_mut() {
            if *last_operation_id == operation_id
                && can_coalesce(last_event)
                && same_snapshot_stream(last_event, &event)
            {
                *last_event = event;
                return;
            }
        }
    }
    updates.push((operation_id, event));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::progress::{
        OperationKind, Phase, ProgressEvent, Step, TransactionActivityPhase, Unit, WorkProgress,
    };

    fn sector_event(current: u64) -> ProgressEvent {
        let mut event = ProgressEvent::new(Phase::Transaction, Step::ProtocolWrite, 2, 5);
        event.operation = OperationKind::Provision;
        event.delivery = crate::application::progress::ProgressDelivery::WorkSnapshot;
        event.work = Some(WorkProgress::new(current, 1_000, Unit::Sectors));
        event.log_policy = LogPolicy::SnapshotOnly;
        event
    }

    #[test]
    fn coalesces_same_high_frequency_snapshot_stream_to_latest() {
        let mut updates = Vec::new();
        for current in 1..=1_000 {
            push_progress_coalesced(&mut updates, 7_u64, sector_event(current));
        }
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].1.work.unwrap().current, 1_000);
    }

    #[test]
    fn reliable_warnings_and_errors_are_never_coalesced() {
        let mut updates = Vec::new();
        let mut warning = sector_event(1);
        warning.severity = Severity::Warning;
        warning.delivery = crate::application::progress::ProgressDelivery::Reliable;
        push_progress_coalesced(&mut updates, 7_u64, warning);

        let mut error = sector_event(2);
        error.severity = Severity::Error;
        push_progress_coalesced(&mut updates, 7_u64, error);

        let mut rollback = sector_event(3);
        rollback.delivery = crate::application::progress::ProgressDelivery::Reliable;
        rollback.work.as_mut().unwrap().activity = Some(TransactionActivityPhase::RollbackWrite);
        push_progress_coalesced(&mut updates, 7_u64, rollback);

        assert_eq!(updates.len(), 3);
    }

    #[test]
    fn phase_step_and_activity_boundaries_are_preserved() {
        let mut updates = Vec::new();
        push_progress_coalesced(&mut updates, 7_u64, sector_event(1));
        let mut readback = sector_event(2);
        readback.work.as_mut().unwrap().activity = Some(TransactionActivityPhase::Readback);
        push_progress_coalesced(&mut updates, 7_u64, readback);
        let mut complete = ProgressEvent::new(Phase::Complete, Step::Completed, 5, 5);
        complete.operation = OperationKind::Provision;
        complete.log_policy = LogPolicy::Append;
        push_progress_coalesced(&mut updates, 7_u64, complete);
        assert_eq!(updates.len(), 3);
    }
}
