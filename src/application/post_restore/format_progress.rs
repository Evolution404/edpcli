use crate::application::progress::{
    FormatStep, LogPolicy, OperationKind, Phase, ProgressEvent, ProgressSpan, Severity, Step,
    TransactionActivity, TransactionActivityPhase, WorkProgress,
};

pub(super) const STAGES: u64 = 10;

pub(super) fn stage(step: FormatStep) -> ProgressEvent {
    let completed = match step {
        FormatStep::VerifyTarget => 0,
        FormatStep::LockAndReopen => 1,
        FormatStep::BuildImage | FormatStep::EncryptImage => 2,
        FormatStep::SyncPreflight => 3,
        FormatStep::Mirror => 4,
        FormatStep::Write => 5,
        FormatStep::Sync => 6,
        FormatStep::Readback => 7,
        FormatStep::VerifyFilesystem => 8,
        FormatStep::Reassess => 9,
        // Rollback is recovery from failure, never forward completion.
        FormatStep::RollbackWrite | FormatStep::RollbackSync | FormatStep::RollbackReadback => 0,
    };
    let phase = match step {
        FormatStep::VerifyTarget | FormatStep::LockAndReopen => Phase::Identity,
        FormatStep::Readback | FormatStep::VerifyFilesystem | FormatStep::Reassess => {
            Phase::Readback
        }
        _ => Phase::Format,
    };
    let mut event = ProgressEvent::new(phase, Step::PostRestoreFormat(step), completed, STAGES);
    event.operation = OperationKind::PostRestoreFormat;
    event.detail = Some(step.label().into());
    if matches!(
        step,
        FormatStep::RollbackWrite | FormatStep::RollbackSync | FormatStep::RollbackReadback
    ) {
        event.severity = Severity::Warning;
    }
    event
}

pub(super) fn activity(activity: TransactionActivity) -> ProgressEvent {
    let step = match activity.phase {
        TransactionActivityPhase::SyncPreflight => FormatStep::SyncPreflight,
        TransactionActivityPhase::Mirror => FormatStep::Mirror,
        TransactionActivityPhase::Write | TransactionActivityPhase::FormatWrite => {
            FormatStep::Write
        }
        TransactionActivityPhase::Sync => FormatStep::Sync,
        TransactionActivityPhase::Readback | TransactionActivityPhase::FormatReadback => {
            FormatStep::Readback
        }
        TransactionActivityPhase::RollbackWrite => FormatStep::RollbackWrite,
        TransactionActivityPhase::RollbackSync => FormatStep::RollbackSync,
        TransactionActivityPhase::RollbackReadback => FormatStep::RollbackReadback,
    };
    let mut event = stage(step);
    if activity.total > 0 {
        let work = WorkProgress::from_activity(activity);
        if event.severity == Severity::Info {
            let completed = event.stage.expect("format stage").current;
            event.overall =
                ProgressSpan::for_index(completed, STAGES).interpolate(work.current, work.total);
        }
        event.delivery = crate::application::progress::ProgressDelivery::WorkSnapshot;
        event.work = Some(work);
        event.log_policy = if activity.current == 0 {
            LogPolicy::AppendOnChange
        } else {
            LogPolicy::SnapshotOnly
        };
    }
    event
}

pub(super) fn completed() -> ProgressEvent {
    let mut event = ProgressEvent::new(Phase::Complete, Step::Completed, STAGES, STAGES);
    event.operation = OperationKind::PostRestoreFormat;
    event.detail = Some("格式化完成，读回校验与分区可用性评估通过".into());
    event.log_policy = LogPolicy::Append;
    event
}

pub(super) fn emit(prompt: &mut dyn crate::application::Prompter, event: ProgressEvent) {
    crate::application::progress::emit_isolated(
        &mut |event| prompt.operation_progress(event),
        event,
    );
}
