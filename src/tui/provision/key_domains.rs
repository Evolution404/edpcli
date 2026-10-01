use super::*;

impl AppState {
    pub(super) fn provision_initialize_password_candidates(&mut self, kind: ProvisionKind) {
        use crate::provision::{KeyDomainRole, SourcePasswordKnowledge};
        use password_verification::{TargetPasswordMode, TargetPasswordModeState};

        self.provision.source_password_edit_dirty = false;
        self.provision.share_source_password_revision = 0;
        self.provision.encrypt_source_password_revision = 0;
        self.provision.target_password_modes = TargetPasswordModeState::default();
        self.provision.form.share_source_password.clear();
        self.provision.form.encrypt_source_password.clear();
        self.provision.form.share_target_password.clear();
        self.provision.form.encrypt_target_password.clear();
        self.provision.form.share_source_knowledge = SourcePasswordKnowledge::Unknown;
        self.provision.form.encrypt_source_knowledge = SourcePasswordKnowledge::Unknown;
        self.provision.share_source_verification = ProvisionPasswordVerificationState::Idle;
        self.provision.encrypt_source_verification = ProvisionPasswordVerificationState::Idle;

        let mode = kind.mode();
        let password =
            String::from_utf8_lossy(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD).into_owned();
        let share_active = matches!(mode, Some(0 | 1 | 3));
        let encrypt_active = matches!(mode, Some(0..=2));
        let share_from_source =
            share_active && self.provision_source_has_password_domain(KeyDomainRole::Share);
        let encrypt_from_source =
            encrypt_active && self.provision_source_has_password_domain(KeyDomainRole::Encrypt);

        if share_active {
            if share_from_source {
                self.provision.form.share_source_password = password.clone();
                self.provision.share_source_verification =
                    ProvisionPasswordVerificationState::Verifying;
            } else {
                self.provision.form.share_target_password = password.clone();
                self.provision.target_password_modes.share = TargetPasswordMode::Explicit;
            }
        }
        if encrypt_active {
            if encrypt_from_source {
                self.provision.form.encrypt_source_password = password.clone();
                self.provision.encrypt_source_verification =
                    ProvisionPasswordVerificationState::Verifying;
            } else {
                self.provision.form.encrypt_target_password = password;
                self.provision.target_password_modes.encrypt = TargetPasswordMode::Explicit;
            }
        }
    }

    pub fn provision_set_planning(&mut self) {
        self.provision_transition_begin_planning();
    }

    pub fn provision_finish_plan(&mut self, result: Result<ProvisionPrepared, String>) {
        match result {
            Ok(prepared) => {
                if let Err(message) = ProvisionConfirmationViewModel::from_prepared(&prepared) {
                    self.provision_transition_plan_failed(format!(
                        "错误: 计划确认投影失败: {message}"
                    ));
                    return;
                }
                if let ProvisionPrepared::Official(official) = &prepared {
                    if let Some(target_plan) = &official.target_plan {
                        for part in &target_plan.partitions {
                            match crate::provision::KeyDomainRole::from_partition_role(
                                part.geometry.role,
                            ) {
                                Some(crate::provision::KeyDomainRole::Share) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.share_source_knowledge = knowledge;
                                    }
                                }
                                Some(crate::provision::KeyDomainRole::Encrypt) => {
                                    if let Some(knowledge) = part.source_password_knowledge {
                                        self.provision.form.encrypt_source_knowledge = knowledge;
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                }
                self.provision.prepared = Some(prepared);
                self.provision_transition_plan_succeeded();
            }
            Err(message) => self.provision_transition_plan_failed(message),
        }
    }
}
