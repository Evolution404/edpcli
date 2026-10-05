//! Apply restore/format completion results to the follow-up workbench.
use super::*;

impl AppState {
    pub fn finish_restore(
        &mut self,
        result: Result<crate::application::post_restore::MetadataRestoreOutcome, String>,
    ) {
        self.shell.critical_operation = false;
        let mut initialize_workbench = false;
        {
            let Some(wizard) = self.restore.wizard.as_mut() else {
                return;
            };
            match result {
                Ok(outcome) => {
                    wizard.stage = WizardStage::PostRestore;
                    wizard.restore_outcome = Some(outcome);
                    wizard.pending_format = None;
                    Self::clear_post_restore_volume_label(wizard);
                    wizard.message = Some(crate::tui::ui::UiMessage::success(
                        "元数据恢复成功；文件系统状态已完成只读检查。",
                    ));
                    self.shell.input_mode = InputMode::Normal;
                    initialize_workbench = true;
                }
                Err(message) => {
                    wizard.stage = WizardStage::Result;
                    if let Some(run) = wizard.run.as_mut() {
                        let mut event = crate::application::progress::ProgressEvent::started(
                            crate::application::progress::OperationKind::Restore,
                            crate::application::progress::Phase::Transaction,
                            crate::application::progress::Step::RestoreWrite,
                            message.clone(),
                        );
                        event.severity = crate::application::progress::Severity::Error;
                        event.log_policy = crate::application::progress::LogPolicy::Append;
                        run.push(event);
                    }
                    wizard.message = Some(crate::tui::ui::UiMessage::error(message));
                    self.shell.input_mode = InputMode::Normal;
                }
            }
        }
        if initialize_workbench {
            self.initialize_post_restore_result_workbench();
        }
    }

    pub fn abort_post_restore_format(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.shell.critical_operation = false;
        if let Some(wizard) = self.restore.wizard.as_mut() {
            Self::record_post_restore_format_result(wizard, Some(message.clone()));
            wizard.stage = WizardStage::PostRestore;
            wizard.pending_format = None;
            Self::clear_post_restore_volume_label(wizard);
            wizard.message = Some(crate::tui::ui::UiMessage::error(message));
            self.shell.input_mode = InputMode::Normal;
        }
    }

    pub(crate) fn finish_post_restore_format_assessed(
        &mut self,
        result: crate::application::post_restore::PostRestoreFormatResult,
        assessment: Option<crate::application::post_restore::PostRestoreAssessment>,
    ) {
        let succeeded = result.result.is_ok();
        self.finish_post_restore_format(result);
        if succeeded {
            if let (Some(assessment), Some(outcome)) = (
                assessment,
                self.restore
                    .wizard
                    .as_mut()
                    .and_then(|wizard| wizard.restore_outcome.as_mut()),
            ) {
                outcome.assessment = assessment;
            }
        }
    }

    pub fn finish_post_restore_format(
        &mut self,
        result: crate::application::post_restore::PostRestoreFormatResult,
    ) {
        use crate::application::post_restore::PostRestorePartitionState;

        self.shell.critical_operation = false;
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        Self::record_post_restore_format_result(wizard, result.result.as_ref().err().cloned());
        wizard.stage = WizardStage::PostRestore;
        wizard.pending_format = None;
        Self::clear_post_restore_volume_label(wizard);
        match result.result {
            Ok(()) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::Usable;
                        partition.detected_filesystem = Some(result.filesystem);
                        partition.detail = "格式化完成并通过读回重新评估".into();
                    }
                }
                wizard.message = Some(crate::tui::ui::UiMessage::success(format!(
                    "分区 {} 格式化完成并重新评估为可用。",
                    result.partition_index
                )));
            }
            Err(message) => {
                wizard.message = Some(crate::tui::ui::UiMessage::error(format!(
                    "分区 {} 格式化失败：{}；元数据恢复仍保持成功。",
                    result.partition_index, message
                )));
            }
        }
        self.shell.input_mode = InputMode::Normal;
    }

    pub fn finish_post_restore_encrypted_format(
        &mut self,
        result: crate::application::post_restore::EncryptedPostRestoreFormatResult,
    ) {
        use crate::application::post_restore::{
            EncryptedPostRestoreError, PostRestorePartitionState,
        };
        use crate::provision::ExistingFileKeyError;

        self.shell.critical_operation = false;
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        Self::record_post_restore_format_result(
            wizard,
            result.result.as_ref().err().map(|error| match error {
                EncryptedPostRestoreError::Operation(message) => message.clone(),
                EncryptedPostRestoreError::FileKey(error) => format!("原密钥验证失败: {error:?}"),
            }),
        );
        wizard.pending_format = None;
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();

        match result.result {
            Ok(()) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::Usable;
                        partition.detected_filesystem = Some(result.filesystem);
                        partition.detail = "原 FileKey 格式化完成并通过读回重新评估".into();
                    }
                }
                wizard.stage = WizardStage::PostRestore;
                Self::clear_post_restore_volume_label(wizard);
                wizard.message = Some(crate::tui::ui::UiMessage::success(format!(
                    "分区 {} 已使用原密钥域格式化并重新评估为可用。",
                    result.partition_index
                )));
                self.shell.input_mode = InputMode::Normal;
            }
            Err(EncryptedPostRestoreError::FileKey(
                ExistingFileKeyError::PasswordRequired | ExistingFileKeyError::PasswordMismatch,
            )) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::PasswordRequired;
                    }
                }
                wizard.pending_format =
                    Some(crate::application::post_restore::PartitionFormatRequest {
                        partition_index: result.partition_index,
                        filesystem: result.filesystem,
                    });
                wizard.stage = WizardStage::PasswordInput;
                wizard.message = Some(crate::tui::ui::UiMessage::warning(
                    "原密码验证失败，请重新输入原密码。",
                ));
                self.shell.input_mode = InputMode::Insert;
            }
            Err(EncryptedPostRestoreError::FileKey(
                ExistingFileKeyError::UnsupportedEncryptMode
                | ExistingFileKeyError::FileKeyCrcMismatch
                | ExistingFileKeyError::MalformedKeyRecord,
            )) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::CryptoMetadataInvalid;
                    }
                }
                wizard.stage = WizardStage::PostRestore;
                wizard.message = Some(crate::tui::ui::UiMessage::error(
                    "密钥记录无法可靠验证；已转为“加密元数据异常”，可选择重建加密分区。",
                ));
                self.shell.input_mode = InputMode::Normal;
            }
            Err(EncryptedPostRestoreError::Operation(message)) => {
                wizard.stage = WizardStage::PostRestore;
                Self::clear_post_restore_volume_label(wizard);
                wizard.message = Some(crate::tui::ui::UiMessage::error(format!(
                    "分区 {} 加密格式化失败：{}；元数据恢复仍保持成功。",
                    result.partition_index, message
                )));
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn abort_post_restore_encrypted_action(&mut self, message: impl Into<String>) {
        self.shell.critical_operation = false;
        if let Some(wizard) = self.restore.wizard.as_mut() {
            wizard.stage = WizardStage::PostRestore;
            wizard.pending_format = None;
            Self::clear_post_restore_volume_label(wizard);
            wizard.secret_input = crate::provision::SecretBytes::default();
            wizard.secret_first = crate::provision::SecretBytes::default();
            wizard.message = Some(crate::tui::ui::UiMessage::error(message));
            self.shell.input_mode = InputMode::Normal;
        }
    }

    pub fn finish_post_restore_reinitialize(
        &mut self,
        result: crate::application::post_restore::EncryptedPartitionReinitializeResult,
    ) {
        use crate::application::post_restore::PostRestorePartitionState;

        self.shell.critical_operation = false;
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        wizard.pending_format = None;
        Self::clear_post_restore_volume_label(wizard);
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();
        wizard.stage = WizardStage::PostRestore;
        self.shell.input_mode = InputMode::Normal;

        match result.result {
            Ok(()) => {
                if let Some(outcome) = wizard.restore_outcome.as_mut() {
                    if let Some(partition) = outcome
                        .assessment
                        .partitions
                        .iter_mut()
                        .find(|partition| partition.index == result.partition_index)
                    {
                        partition.state = PostRestorePartitionState::Usable;
                        partition.detected_filesystem = Some(result.filesystem);
                        partition.detail = "新密钥域与新空文件系统已通过读回验证".into();
                    }
                }
                wizard.message = Some(crate::tui::ui::UiMessage::success(format!(
                    "分区 {} 已重建密钥域并重新评估为可用。",
                    result.partition_index
                )));
            }
            Err(message) => {
                wizard.message = Some(crate::tui::ui::UiMessage::error(format!(
                    "分区 {} 重建失败：{}；元数据恢复仍保持成功。",
                    result.partition_index, message
                )));
            }
        }
    }
}
