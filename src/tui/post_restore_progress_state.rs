//! Independent progress lifecycle for post-restore partition formatting.

use super::{AppState, WizardStage, WizardState};

impl AppState {
    pub(crate) fn post_restore_format_running(&self) -> bool {
        self.wizard()
            .is_some_and(|wizard| wizard.stage == WizardStage::Formatting)
    }

    pub(super) fn start_post_restore_format_progress(
        wizard: &mut WizardState,
        request: &crate::application::post_restore::PartitionFormatRequest,
    ) {
        use crate::application::progress::{
            FormatStep, OperationKind, OperationRunState, Phase, ProgressEvent, Step,
        };
        let mut run = OperationRunState::new(
            OperationKind::PostRestoreFormat,
            format!(
                "disk{} · 分区 {} · {}",
                wizard.disk,
                request.partition_index,
                request.filesystem.display_name()
            ),
        );
        run.push(ProgressEvent::started(
            OperationKind::PostRestoreFormat,
            Phase::Identity,
            Step::PostRestoreFormat(FormatStep::VerifyTarget),
            "正在复核目标身份与分区状态",
        ));
        wizard.format_run = Some(run);
    }

    pub fn set_post_restore_format_progress(
        &mut self,
        event: crate::application::progress::ProgressEvent,
    ) {
        if event.operation != crate::application::progress::OperationKind::PostRestoreFormat {
            return;
        }
        if let Some(wizard) = self.restore.wizard.as_mut() {
            if wizard.stage == WizardStage::Formatting {
                if let Some(run) = wizard.format_run.as_mut() {
                    run.push(event);
                }
            }
        }
    }

    pub(super) fn record_post_restore_format_result(
        wizard: &mut WizardState,
        error: Option<String>,
    ) {
        use crate::application::progress::{OperationKind, Phase, ProgressEvent, Severity, Step};
        let Some(run) = wizard.format_run.as_mut() else {
            return;
        };
        if let Some(message) = error {
            let phase = run
                .latest
                .as_ref()
                .map_or(Phase::Format, |event| event.phase);
            let step = run
                .latest
                .as_ref()
                .map_or(Step::Completed, |event| event.step);
            let mut event =
                ProgressEvent::started(OperationKind::PostRestoreFormat, phase, step, message);
            event.severity = Severity::Error;
            run.push(event);
        } else if !run
            .latest
            .as_ref()
            .is_some_and(|event| event.step == Step::Completed)
        {
            let total = run
                .latest
                .as_ref()
                .and_then(|event| event.stage)
                .map_or(1, |stage| stage.total);
            let mut event = ProgressEvent::new(Phase::Complete, Step::Completed, total, total);
            event.operation = OperationKind::PostRestoreFormat;
            event.detail = Some("格式化完成并重新评估为可用".into());
            run.push(event);
        }
    }
}
