//! Pure restore workflow transitions on AppState.
use crate::tui::state::*;
impl AppState {
    pub fn wizard(&self) -> Option<&WizardState> {
        self.restore.wizard.as_ref()
    }

    pub fn begin_write_wizard_for_identity(
        &mut self,
        kind: WriteKind,
        disk: u32,
        backup: Option<std::path::PathBuf>,
        expected_identity: Option<crate::application::media_identity::MediaIdentityResumePin>,
    ) -> bool {
        if self.shell.critical_operation {
            self.set_warning_notice("关键操作仍在执行，完成前不能启动其他任务。".to_string());
            return false;
        }
        let stage = WizardStage::Confirm;
        self.shell.input_mode = if kind == WriteKind::Restore {
            InputMode::Confirm
        } else {
            InputMode::Normal
        };
        self.shell.confirmation_offset = 0;
        self.restore.wizard = Some(WizardState {
            stage,
            kind,
            disk,
            backup,
            expected_identity,
            confirmation: String::new(),
            message: None,
            restore_outcome: None,
            post_restore_workbench: crate::tui::result_workbench::ResultWorkbenchState::default(),
            pending_format: None,
            volume_label_input: String::new(),
            volume_label_target: None,
            secret_input: crate::provision::SecretBytes::default(),
            secret_first: crate::provision::SecretBytes::default(),
            run: None,
            format_run: None,
        });
        true
    }

    pub fn confirm_backup_create(&mut self) -> Option<WriteIntent> {
        let wizard = self.restore.wizard.as_mut()?;
        if wizard.kind != WriteKind::BackupCreate || wizard.stage != WizardStage::Confirm {
            return None;
        }
        let intent = WriteIntent {
            kind: wizard.kind,
            disk: wizard.disk,
            backup: None,
            expected_identity: wizard.expected_identity.clone(),
        };
        wizard.stage = WizardStage::Running;
        wizard.message = Some(crate::tui::ui::UiMessage::progress(
            "正在只读采集并创建元数据备份。",
        ));
        let mut run = crate::application::progress::OperationRunState::new(
            crate::application::progress::OperationKind::Backup,
            format!("disk{}", wizard.disk),
        );
        run.push(crate::application::progress::ProgressEvent::started(
            crate::application::progress::OperationKind::Backup,
            crate::application::progress::Phase::Backup,
            crate::application::progress::Step::BackupCreate,
            "正在只读采集设备元数据",
        ));
        wizard.run = Some(run);
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(intent)
    }

    pub fn push_wizard_confirmation(&mut self, ch: char) {
        if let Some(wizard) = self.restore.wizard.as_mut() {
            let media_write_confirmation = match wizard.stage {
                WizardStage::Confirm => wizard.kind == WriteKind::Restore,
                WizardStage::FormatConfirm
                | WizardStage::EncryptedFormatConfirm
                | WizardStage::ReinitializeConfirm => true,
                _ => false,
            };
            if media_write_confirmation && wizard.confirmation.len() < 16 {
                wizard.confirmation.push(ch);
                wizard.message = None;
            }
        }
    }

    pub fn backspace_wizard_confirmation(&mut self) {
        if let Some(wizard) = self.restore.wizard.as_mut() {
            let media_write_confirmation = match wizard.stage {
                WizardStage::Confirm => wizard.kind == WriteKind::Restore,
                WizardStage::FormatConfirm
                | WizardStage::EncryptedFormatConfirm
                | WizardStage::ReinitializeConfirm => true,
                _ => false,
            };
            if media_write_confirmation {
                wizard.confirmation.pop();
                wizard.message = None;
            }
        }
    }

    pub fn clear_wizard_confirmation(&mut self) {
        if let Some(wizard) = self.restore.wizard.as_mut() {
            wizard.confirmation.clear();
            wizard.message = None;
        }
    }

    pub(crate) fn viewport_size(&self) -> ratatui::layout::Size {
        self.shell.viewport_size
    }
    pub(crate) fn help_scroll(&self) -> usize {
        self.shell.help_scroll
    }
    pub(crate) fn set_help_scroll(&mut self, offset: usize) {
        self.shell.help_scroll = offset;
    }

    pub(crate) fn write_confirmation_ready(&mut self) -> bool {
        if crate::tui::ui::confirmation::write_confirmation_fits(self.shell.viewport_size) {
            return true;
        }
        let message =
            crate::tui::ui::UiMessage::warning("窗口过小，禁止写入。请扩大至 40×18；Esc 取消。");
        if let Some(wizard) = self.restore.wizard.as_mut() {
            wizard.message = Some(message.clone());
        }
        self.provision.message = Some(message);
        false
    }

    pub(crate) fn confirmation_target(&self, disk: u32) -> String {
        self.devices()
            .iter()
            .find(|row| row.disk == disk)
            .map(|row| format!("disk{disk} · {}", crate::common::fmt_capacity(row.size)))
            .unwrap_or_else(|| format!("disk{disk}"))
    }

    pub(crate) fn confirmation_offset(&self) -> usize {
        self.shell.confirmation_offset
    }

    pub(crate) fn scroll_confirmation_details(&mut self, reverse: bool) {
        if reverse {
            self.shell.confirmation_offset = self.shell.confirmation_offset.saturating_sub(4);
        } else {
            self.shell.confirmation_offset = self
                .shell
                .confirmation_offset
                .saturating_add(4)
                .min(u16::MAX as usize);
        }
    }

    pub fn submit_wizard_confirmation(&mut self) -> Option<WriteIntent> {
        if !self.write_confirmation_ready() {
            return None;
        }
        let wizard = self.restore.wizard.as_mut()?;
        if wizard.stage != WizardStage::Confirm || wizard.kind != WriteKind::Restore {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some(crate::tui::ui::UiMessage::warning(
                "必须精确输入 YES 才会开始向目标设备写入",
            ));
            return None;
        }
        let intent = WriteIntent {
            kind: wizard.kind,
            disk: wizard.disk,
            backup: wizard.backup.clone(),
            expected_identity: wizard.expected_identity.clone(),
        };
        wizard.stage = WizardStage::Running;
        wizard.message = Some(crate::tui::ui::UiMessage::progress(
            "关键写盘阶段进行中，不可中断",
        ));
        let mut run = crate::application::progress::OperationRunState::new(
            crate::application::progress::OperationKind::Restore,
            format!("disk{}", wizard.disk),
        );
        run.push(crate::application::progress::ProgressEvent::started(
            crate::application::progress::OperationKind::Restore,
            crate::application::progress::Phase::Backup,
            crate::application::progress::Step::RestoreVerification,
            "正在校验备份并固定恢复目标",
        ));
        wizard.run = Some(run);
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(intent)
    }

    pub fn set_write_progress(&mut self, event: crate::application::WriteEvent) {
        if let Some(wizard) = self.restore.wizard.as_mut() {
            if wizard.stage == WizardStage::Running {
                if let Some(run) = wizard.run.as_mut() {
                    let operation = match wizard.kind {
                        WriteKind::BackupCreate => {
                            crate::application::progress::OperationKind::Backup
                        }
                        WriteKind::Restore => crate::application::progress::OperationKind::Restore,
                    };
                    run.push(crate::application::progress::project_write_event(
                        operation, &event,
                    ));
                }
            }
        }
    }

    pub fn finish_write(&mut self, result: Result<(), String>) {
        self.shell.critical_operation = false;
        if let Some(wizard) = self.restore.wizard.as_mut() {
            wizard.stage = WizardStage::Result;
            if let Err(message) = result.as_ref() {
                if let Some(run) = wizard.run.as_mut() {
                    let mut event = crate::application::progress::ProgressEvent::started(
                        crate::application::progress::OperationKind::Backup,
                        crate::application::progress::Phase::Backup,
                        crate::application::progress::Step::BackupCreate,
                        message.clone(),
                    );
                    event.severity = crate::application::progress::Severity::Error;
                    event.log_policy = crate::application::progress::LogPolicy::Append;
                    run.push(event);
                }
            }
            wizard.message = Some(match result {
                Ok(()) if wizard.kind == WriteKind::BackupCreate => {
                    crate::tui::ui::UiMessage::success("备份创建完成；备份列表已刷新")
                }
                Ok(()) => crate::tui::ui::UiMessage::success("操作完成，安全链全部通过"),
                Err(message) => crate::tui::ui::UiMessage::error(message),
            });
        }
    }
}
