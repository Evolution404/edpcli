//! A latest-value mailbox before the unbounded result channel. Only boundaries,
//! diagnostics and final results enter that channel, never every sector snapshot.
use super::*;
use crate::application::progress::ProgressEvent;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ProgressKind {
    Format,
    Provision,
}
pub(super) type ProgressSlot = Arc<Mutex<Option<(OperationId, ProgressKind, ProgressEvent)>>>;

#[derive(Clone)]
pub(super) struct ProgressPublisher {
    tx: Sender<WorkerResult>,
    slot: ProgressSlot,
    operation_id: OperationId,
    kind: ProgressKind,
    retention: Arc<Mutex<crate::application::progress::ProgressRetention>>,
}
impl ProgressPublisher {
    pub(super) fn new(hub: &TaskHub, operation_id: OperationId, kind: ProgressKind) -> Self {
        Self {
            tx: hub.tx.clone(),
            slot: hub.progress_slot.clone(),
            operation_id,
            kind,
            retention: Arc::new(Mutex::new(
                crate::application::progress::ProgressRetention::default(),
            )),
        }
    }
    fn send(&self, operation_id: OperationId, kind: ProgressKind, event: ProgressEvent) {
        if !self
            .retention
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(&event)
        {
            return;
        }
        self.send_direct(operation_id, kind, event);
    }
    fn send_direct(&self, operation_id: OperationId, kind: ProgressKind, event: ProgressEvent) {
        let message = match kind {
            ProgressKind::Format => WorkerResult::PostRestoreFormatProgress {
                operation_id,
                event,
            },
            ProgressKind::Provision => WorkerResult::Provision(ProvisionWorkerResult::Progress {
                operation_id,
                event,
            }),
        };
        let _ = self.tx.send(message);
    }
    pub(super) fn publish(&self, event: ProgressEvent) {
        let mut pending = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        let snapshot = super::super::progress_transport::can_coalesce(&event)
            && event
                .work
                .is_some_and(|work| work.current > 0 && work.current < work.total);
        let same = pending.as_ref().is_some_and(|(id, kind, previous)| {
            *id == self.operation_id
                && *kind == self.kind
                && super::super::progress_transport::same_snapshot_stream(previous, &event)
        });
        if !snapshot || !same {
            if let Some((id, kind, previous)) = pending.take() {
                self.send(id, kind, previous);
            }
        }
        if snapshot {
            *pending = Some((self.operation_id, self.kind, event));
        } else {
            self.send(self.operation_id, self.kind, event);
        }
    }
    pub(super) fn flush(&self) {
        let mut pending = self.slot.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((id, kind, event)) = pending.take() {
            self.send(id, kind, event);
        }
        let notice = self
            .retention
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .finish();
        if let Some(event) = notice {
            self.send_direct(self.operation_id, self.kind, event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::progress::{
        LogPolicy, OperationKind, Phase, Severity, Step, TransactionActivityPhase, Unit,
        WorkProgress,
    };
    fn event(current: u64) -> ProgressEvent {
        let mut event = ProgressEvent::new(Phase::Transaction, Step::ProtocolWrite, 1, 2);
        event.operation = OperationKind::Provision;
        event.log_policy = LogPolicy::SnapshotOnly;
        event.delivery = crate::application::progress::ProgressDelivery::WorkSnapshot;
        event.work = Some(WorkProgress::new(current, 100_001, Unit::Sectors));
        event
    }
    #[test]
    fn paused_ui_keeps_one_snapshot_and_no_sector_queue() {
        let mut hub = TaskHub::new();
        let id = hub.begin_operation().unwrap();
        let publisher = ProgressPublisher::new(&hub, id, ProgressKind::Provision);
        for current in 1..=100_000 {
            publisher.publish(event(current));
        }
        assert!(hub.rx.try_recv().is_err());
        let updates = hub.poll();
        assert_eq!(updates.provision.progress.len(), 1);
        assert_eq!(
            updates.provision.progress[0].1.work.unwrap().current,
            100_000
        );
    }
    #[test]
    fn rollback_snapshots_preserve_error_phase_boundary_and_result() {
        let mut hub = TaskHub::new();
        let id = hub.begin_operation().unwrap();
        let publisher = ProgressPublisher::new(&hub, id, ProgressKind::Provision);
        publisher.publish(event(5));
        let mut error = event(6);
        error.severity = Severity::Error;
        publisher.publish(error);
        for phase in [
            TransactionActivityPhase::RollbackWrite,
            TransactionActivityPhase::RollbackReadback,
        ] {
            for current in 1..=100_000 {
                let mut rollback = event(current);
                rollback.severity = Severity::Warning;
                rollback.work.as_mut().unwrap().activity = Some(phase);
                publisher.publish(rollback);
            }
        }
        publisher.flush();
        hub.tx
            .send(WorkerResult::Write {
                operation_id: id,
                result: Err("original failure".into()),
            })
            .unwrap();
        let updates = hub.poll();
        assert_eq!(updates.provision.progress.len(), 4);
        assert_eq!(updates.provision.progress[0].1.work.unwrap().current, 5);
        assert_eq!(updates.provision.progress[1].1.severity, Severity::Error);
        assert_eq!(
            updates.provision.progress[2].1.work.unwrap().activity,
            Some(TransactionActivityPhase::RollbackWrite)
        );
        assert_eq!(
            updates.provision.progress[3].1.work.unwrap().current,
            100_000
        );
        assert!(updates
            .write
            .unwrap()
            .1
            .unwrap_err()
            .to_string()
            .contains("original failure"));
        assert!(hub.active_operation().is_none());
    }
}
