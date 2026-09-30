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
            TransactionActivityPhase::Mirror => step_span.subspan(0, 1_000),
            TransactionActivityPhase::Write => step_span.subspan(1_000, 6_000),
            TransactionActivityPhase::Readback => step_span.subspan(6_000, 10_000),
            TransactionActivityPhase::FormatWrite => step_span.subspan(0, 7_500),
            TransactionActivityPhase::FormatReadback => step_span.subspan(7_500, 10_000),
            TransactionActivityPhase::RollbackWrite
            | TransactionActivityPhase::RollbackReadback => step_span.subspan(0, 0),
        };
        event.overall = work_span.interpolate(work.current, work.total);
        event.work = Some(work);
        event.log_policy = LogPolicy::AppendOnChange;
        if matches!(
            activity.phase,
            TransactionActivityPhase::RollbackWrite | TransactionActivityPhase::RollbackReadback
        ) {
            event.severity = Severity::Warning;
        }
    } else {
        event.log_policy = LogPolicy::AppendOnChange;
    }
    event
}
