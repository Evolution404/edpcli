//! Restore follow-up inputs, passwords, volume labels and confirmation transitions.
use super::*;

impl AppState {
    pub(super) fn clear_post_restore_volume_label(wizard: &mut WizardState) {
        wizard.volume_label_input.clear();
        wizard.volume_label_target = None;
    }

    fn selected_post_restore_volume_label(wizard: &WizardState, partition_index: u32) -> String {
        wizard
            .restore_outcome
            .as_ref()
            .and_then(|outcome| {
                outcome
                    .partitions
                    .iter()
                    .find(|partition| partition.index == partition_index)
            })
            .and_then(|partition| partition.volume_label_hint.clone())
            .unwrap_or_default()
    }

    fn begin_volume_label_input(
        wizard: &mut WizardState,
        target: PostRestoreLabelTarget,
        request: crate::application::post_restore::PartitionFormatRequest,
    ) {
        wizard.volume_label_input =
            Self::selected_post_restore_volume_label(wizard, request.partition_index);
        wizard.volume_label_target = Some(target);
        wizard.pending_format = Some(request);
        wizard.confirmation.clear();
        wizard.message = None;
        wizard.stage = WizardStage::VolumeLabelInput;
    }

    pub fn begin_selected_post_restore_action(&mut self) {
        use crate::application::post_restore::PostRestorePartitionState;

        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::PostRestore {
            return;
        }
        let Some(outcome) = wizard.restore_outcome.as_ref() else {
            return;
        };
        let Some(selected) = wizard.active_post_restore_partition_index() else {
            wizard.message = Some(crate::tui::ui::UiMessage::warning(
                "当前激活区域不是可处理分区。",
            ));
            return;
        };
        let Some(partition) = outcome.assessment.partitions.get(selected) else {
            return;
        };
        let state = partition.state;
        let requires_original_key = partition.requires_original_key;
        let format_request = if matches!(
            state,
            PostRestorePartitionState::NeedsFormat
                | PostRestorePartitionState::PasswordRequired
                | PostRestorePartitionState::CryptoMetadataInvalid
        ) {
            match crate::application::post_restore::PartitionFormatRequest::for_post_restore_partition(
                partition,
            ) {
                Ok(request) => Some(request),
                Err(error) => {
                    wizard.message = Some(crate::tui::ui::UiMessage::warning(error.to_string()));
                    return;
                }
            }
        } else {
            None
        };

        match state {
            PostRestorePartitionState::NeedsFormat if !requires_original_key => {
                let request = format_request
                    .clone()
                    .expect("format-capable state has request");
                Self::begin_volume_label_input(
                    wizard,
                    PostRestoreLabelTarget::PlainFormat,
                    request,
                );
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::NeedsFormat => {
                let request = format_request
                    .clone()
                    .expect("format-capable state has request");
                wizard.secret_input = crate::provision::SecretBytes::default();
                Self::begin_volume_label_input(
                    wizard,
                    PostRestoreLabelTarget::EncryptedFormat,
                    request,
                );
                wizard.message = Some(crate::tui::ui::UiMessage::success(
                    "原密钥域已验证；可确认或修改恢复后的卷标。",
                ));
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::PasswordRequired => {
                let request = format_request
                    .clone()
                    .expect("format-capable state has request");
                wizard.volume_label_input =
                    Self::selected_post_restore_volume_label(wizard, request.partition_index);
                wizard.volume_label_target = Some(PostRestoreLabelTarget::EncryptedFormat);
                wizard.pending_format = Some(request);
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = None;
                wizard.stage = WizardStage::PasswordInput;
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::CryptoMetadataInvalid => {
                let request = format_request
                    .clone()
                    .expect("format-capable state has request");
                wizard.volume_label_input =
                    Self::selected_post_restore_volume_label(wizard, request.partition_index);
                wizard.volume_label_target = Some(PostRestoreLabelTarget::Reinitialize);
                wizard.pending_format = Some(request);
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = Some(crate::tui::ui::UiMessage::warning(
                    "将清空并重建该加密分区：旧 FileKey 与旧密码会失效。",
                ));
                wizard.stage = WizardStage::ReinitializePassword;
                self.shell.input_mode = InputMode::Insert;
            }
            PostRestorePartitionState::Usable => {
                let message = if partition.role.as_deref() == Some("compatibility_reserve") {
                    "模式2兼容保留区不承载文件系统，无需格式化。"
                } else {
                    "该分区已经可用，不需要执行破坏性操作。"
                };
                wizard.message = Some(crate::tui::ui::UiMessage::info(message));
            }
            PostRestorePartitionState::Unsupported => {
                wizard.message = Some(crate::tui::ui::UiMessage::warning(
                    "当前状态无法可靠处理，拒绝猜测执行。",
                ));
            }
        }
    }

    fn secret_char_count(secret: &crate::provision::SecretBytes) -> usize {
        secret
            .as_bytes()
            .iter()
            .filter(|byte| (**byte & 0b1100_0000) != 0b1000_0000)
            .count()
    }

    pub fn wizard_secret_len(&self) -> usize {
        self.restore
            .wizard
            .as_ref()
            .map_or(0, |wizard| Self::secret_char_count(&wizard.secret_input))
    }

    pub fn push_wizard_secret_char(&mut self, ch: char) {
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        if !matches!(
            wizard.stage,
            WizardStage::PasswordInput
                | WizardStage::ReinitializePassword
                | WizardStage::ReinitializePasswordConfirm
        ) || wizard.secret_input.as_bytes().len() >= 128
        {
            return;
        }
        let mut bytes = wizard.secret_input.as_bytes().to_vec();
        let mut encoded = [0u8; 4];
        bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        wizard.secret_input = crate::provision::SecretBytes::new(&bytes);
        bytes.fill(0);
        encoded.fill(0);
        wizard.message = None;
    }

    pub fn backspace_wizard_secret(&mut self) {
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        if !matches!(
            wizard.stage,
            WizardStage::PasswordInput
                | WizardStage::ReinitializePassword
                | WizardStage::ReinitializePasswordConfirm
        ) {
            return;
        }
        let mut bytes = wizard.secret_input.as_bytes().to_vec();
        if !bytes.is_empty() {
            let mut cut = bytes.len() - 1;
            while cut > 0 && (bytes[cut] & 0b1100_0000) == 0b1000_0000 {
                cut -= 1;
            }
            bytes.truncate(cut);
        }
        wizard.secret_input = crate::provision::SecretBytes::new(&bytes);
        bytes.fill(0);
        wizard.message = None;
    }

    pub fn submit_wizard_secret(&mut self) {
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        if wizard.secret_input.is_empty() {
            wizard.message = Some(crate::tui::ui::UiMessage::warning("密码不能为空。"));
            return;
        }
        match wizard.stage {
            WizardStage::PasswordInput => {
                wizard.confirmation.clear();
                wizard.message = None;
                wizard.stage = WizardStage::VolumeLabelInput;
                self.shell.input_mode = InputMode::Insert;
            }
            WizardStage::ReinitializePassword => {
                wizard.secret_first = wizard.secret_input.clone();
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.message = None;
                wizard.stage = WizardStage::ReinitializePasswordConfirm;
                self.shell.input_mode = InputMode::Insert;
            }
            WizardStage::ReinitializePasswordConfirm => {
                if wizard.secret_first != wizard.secret_input {
                    wizard.secret_first = crate::provision::SecretBytes::default();
                    wizard.secret_input = crate::provision::SecretBytes::default();
                    wizard.message = Some(crate::tui::ui::UiMessage::warning(
                        "两次输入的新密码不一致，请重新输入。",
                    ));
                    wizard.stage = WizardStage::ReinitializePassword;
                    self.shell.input_mode = InputMode::Insert;
                    return;
                }
                wizard.confirmation.clear();
                wizard.message = None;
                wizard.stage = WizardStage::VolumeLabelInput;
                self.shell.input_mode = InputMode::Insert;
            }
            _ => {}
        }
    }

    pub fn push_wizard_volume_label_char(&mut self, ch: char) {
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::VolumeLabelInput
            || wizard.volume_label_input.len() >= 128
            || ch.is_control()
        {
            return;
        }
        wizard.volume_label_input.push(ch);
        wizard.message = None;
    }

    pub fn backspace_wizard_volume_label(&mut self) {
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        if wizard.stage == WizardStage::VolumeLabelInput {
            wizard.volume_label_input.pop();
            wizard.message = None;
        }
    }

    pub fn submit_wizard_volume_label(&mut self) {
        let Some(wizard) = self.restore.wizard.as_mut() else {
            return;
        };
        if wizard.stage != WizardStage::VolumeLabelInput {
            return;
        }
        let Some(request) = wizard.pending_format.as_ref() else {
            wizard.message = Some(crate::tui::ui::UiMessage::error("缺少恢复后格式化请求。"));
            return;
        };
        if let Err(message) =
            crate::filesystem::validate_volume_label(request.filesystem, &wizard.volume_label_input)
        {
            wizard.message = Some(crate::tui::ui::UiMessage::error(message));
            return;
        }
        wizard.confirmation.clear();
        wizard.message = None;
        wizard.stage = match wizard.volume_label_target {
            Some(PostRestoreLabelTarget::PlainFormat) => WizardStage::FormatConfirm,
            Some(PostRestoreLabelTarget::EncryptedFormat) => WizardStage::EncryptedFormatConfirm,
            Some(PostRestoreLabelTarget::Reinitialize) => WizardStage::ReinitializeConfirm,
            None => {
                wizard.message = Some(crate::tui::ui::UiMessage::error("缺少卷标输入目标。"));
                return;
            }
        };
        self.shell.input_mode = InputMode::Confirm;
    }

    pub fn cancel_post_restore_volume_label(&mut self) {
        if let Some(wizard) = self.restore.wizard.as_mut() {
            if wizard.stage == WizardStage::VolumeLabelInput {
                wizard.stage = WizardStage::PostRestore;
                wizard.confirmation.clear();
                wizard.pending_format = None;
                wizard.volume_label_input.clear();
                wizard.volume_label_target = None;
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = None;
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn cancel_post_restore_secret_flow(&mut self) {
        if let Some(wizard) = self.restore.wizard.as_mut() {
            if matches!(
                wizard.stage,
                WizardStage::PasswordInput
                    | WizardStage::EncryptedFormatConfirm
                    | WizardStage::ReinitializePassword
                    | WizardStage::ReinitializePasswordConfirm
                    | WizardStage::ReinitializeConfirm
            ) {
                wizard.stage = WizardStage::PostRestore;
                wizard.confirmation.clear();
                wizard.pending_format = None;
                wizard.volume_label_input.clear();
                wizard.volume_label_target = None;
                wizard.secret_input = crate::provision::SecretBytes::default();
                wizard.secret_first = crate::provision::SecretBytes::default();
                wizard.message = None;
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn submit_encrypted_format_confirmation(
        &mut self,
    ) -> Option<EncryptedPostRestoreFormatIntent> {
        if !self.write_confirmation_ready() {
            return None;
        }
        let wizard = self.restore.wizard.as_mut()?;
        if wizard.stage != WizardStage::EncryptedFormatConfirm {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some(crate::tui::ui::UiMessage::warning(
                "开始加密格式化写入前必须独立输入 YES。",
            ));
            return None;
        }
        let outcome = wizard.restore_outcome.clone()?;
        let request = wizard.pending_format.clone()?;
        let password = if wizard.secret_input.is_empty() {
            None
        } else {
            Some(wizard.secret_input.clone())
        };
        let volume_label = wizard.volume_label_input.clone();
        wizard.confirmation.clear();
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();
        wizard.stage = WizardStage::Formatting;
        Self::start_post_restore_format_progress(wizard, &request);
        wizard.message = Some(crate::tui::ui::UiMessage::progress(
            "正在使用原 FileKey 创建新的空加密文件系统。",
        ));
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(EncryptedPostRestoreFormatIntent {
            disk: wizard.disk,
            outcome,
            request,
            password,
            volume_label,
        })
    }

    pub fn submit_reinitialize_confirmation(&mut self) -> Option<PostRestoreReinitializeIntent> {
        if !self.write_confirmation_ready() {
            return None;
        }
        let wizard = self.restore.wizard.as_mut()?;
        if wizard.stage != WizardStage::ReinitializeConfirm {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some(crate::tui::ui::UiMessage::warning(
                "开始重建加密分区写入前必须独立输入 YES。",
            ));
            return None;
        }
        let outcome = wizard.restore_outcome.clone()?;
        let format = wizard.pending_format.clone()?;
        let request = crate::application::post_restore::EncryptedPartitionReinitializeRequest::new(
            format.partition_index,
            wizard.secret_first.as_bytes(),
            wizard.secret_input.as_bytes(),
        )
        .ok()?;
        let volume_label = wizard.volume_label_input.clone();
        wizard.confirmation.clear();
        wizard.secret_input = crate::provision::SecretBytes::default();
        wizard.secret_first = crate::provision::SecretBytes::default();
        wizard.stage = WizardStage::Reinitializing;
        wizard.message = Some(crate::tui::ui::UiMessage::progress(
            "正在生成新 FileKey、更新密钥记录并创建新的空加密文件系统。",
        ));
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(PostRestoreReinitializeIntent {
            disk: wizard.disk,
            outcome,
            request,
            filesystem: format.filesystem,
            volume_label,
        })
    }

    pub fn cancel_post_restore_format(&mut self) {
        if let Some(wizard) = self.restore.wizard.as_mut() {
            if wizard.stage == WizardStage::FormatConfirm {
                wizard.stage = WizardStage::PostRestore;
                wizard.confirmation.clear();
                wizard.pending_format = None;
                Self::clear_post_restore_volume_label(wizard);
                wizard.message = None;
                self.shell.input_mode = InputMode::Normal;
            }
        }
    }

    pub fn submit_post_restore_format_confirmation(&mut self) -> Option<PostRestoreFormatIntent> {
        if !self.write_confirmation_ready() {
            return None;
        }
        let wizard = self.restore.wizard.as_mut()?;
        if wizard.stage != WizardStage::FormatConfirm {
            return None;
        }
        if wizard.confirmation != "YES" {
            wizard.message = Some(crate::tui::ui::UiMessage::warning(
                "开始格式化写入前必须再次精确输入 YES。",
            ));
            return None;
        }
        let outcome = wizard.restore_outcome.clone()?;
        let request = wizard.pending_format.clone()?;
        let volume_label = wizard.volume_label_input.clone();
        wizard.stage = WizardStage::Formatting;
        Self::start_post_restore_format_progress(wizard, &request);
        wizard.confirmation.clear();
        wizard.message = Some(crate::tui::ui::UiMessage::progress(
            "正在创建新的空文件系统；元数据恢复结果保持成功。",
        ));
        self.shell.input_mode = InputMode::Normal;
        self.shell.critical_operation = true;
        Some(PostRestoreFormatIntent {
            disk: wizard.disk,
            outcome,
            request,
            volume_label,
        })
    }
}
