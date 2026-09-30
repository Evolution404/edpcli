use super::*;

impl AppState {
    pub(super) fn provision_initialize_password_candidates(&mut self, kind: ProvisionKind) {
        self.provision.source_password_edit_dirty = false;
        self.provision.share_source_password_revision = 0;
        self.provision.encrypt_source_password_revision = 0;
        self.provision.target_password_edits =
            password_verification::TargetPasswordEditState::default();
        let mode = kind.mode();
        let password =
            String::from_utf8_lossy(crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD).into_owned();
        let share_active = matches!(mode, Some(0 | 1 | 3));
        let encrypt_active = matches!(mode, Some(0..=2));
        if share_active {
            self.provision.form.share_source_password = password.clone();
        }
        if encrypt_active {
            self.provision.form.encrypt_source_password = password;
        }
        self.provision.share_source_verification = if share_active {
            ProvisionPasswordVerificationState::Verifying
        } else {
            ProvisionPasswordVerificationState::Idle
        };
        self.provision.encrypt_source_verification = if encrypt_active {
            ProvisionPasswordVerificationState::Verifying
        } else {
            ProvisionPasswordVerificationState::Idle
        };
    }

    pub fn provision_set_planning(&mut self) {
        self.provision_transition_begin_planning();
    }

    pub fn provision_finish_plan(&mut self, result: Result<ProvisionPrepared, String>) {
        match result {
            Ok(prepared) => {
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
